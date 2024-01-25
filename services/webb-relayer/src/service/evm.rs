use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use webb::evm::ethers::prelude::TimeLag;
use webb_event_watcher_traits::{
    BridgeWatcher, EVMEventWatcher as EventWatcher,
};
use webb_relayer_types::{EthersClient, EthersTimeLagClient};

use webb_ew_evm::signature_bridge_watcher::{
    SignatureBridgeContractWatcher, SignatureBridgeContractWrapper,
    SignatureBridgeGovernanceOwnershipTransferredHandler,
};
use webb_ew_evm::vanchor::{
    VAnchorDepositHandler, VAnchorEncryptedOutputHandler, VAnchorLeavesHandler,
};
use webb_ew_evm::{VAnchorContractWatcher, VAnchorContractWrapper};
use webb_proposal_signing_backends::queue::{self, policy};
use webb_proposals::TypedChainId;
use webb_relayer_config::evm::{
    Contract, SignatureBridgeContractConfig, SmartAnchorUpdatesConfig,
    VAnchorContractConfig,
};
use webb_relayer_context::RelayerContext;

use webb_relayer_handlers::routes::fee_info::handle_evm_fee_info;
use webb_relayer_handlers::routes::{
    encrypted_outputs, leaves, metric, private_tx_withdraw, transaction_status,
};
use webb_relayer_tx_queue::evm::TxQueue;

use super::make_proposal_signing_backend;
use super::ProposalSigningBackendSelector;

/// Type alias for providers
pub type Client = EthersClient;
/// Type alias for Timelag provider
pub type TimeLagClient = EthersTimeLagClient;

/// Setup and build all the EVM web services and handlers.
pub fn build_web_services() -> Router<Arc<RelayerContext>> {
    Router::new()
        .route(
            "/leaves/evm/:chain_id/:contract",
            get(leaves::handle_leaves_cache_evm),
        )
        .route(
            "/send/evm/:chain_id/:contract",
            post(private_tx_withdraw::handle_private_tx_withdraw_evm),
        )
        .route(
            "/tx/evm/:chain_id/:item_key",
            get(transaction_status::handle_transaction_status_evm),
        )
        .route(
            "/encrypted_outputs/evm/:chain_id/:contract_address",
            get(encrypted_outputs::handle_encrypted_outputs_cache_evm),
        )
        .route(
            "/metrics/evm/:chain_id/:contract",
            get(metric::handle_evm_metric_info),
        )
        // for backward compatibility
        .route("/metrics", get(metric::handle_metric_info))
        .route(
            "/fee_info/evm/:chain_id/:vanchor/:gas_amount",
            get(handle_evm_fee_info),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

/// Fires up all background services for all EVM chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    for chain_config in ctx.config.evm.values() {
        if !chain_config.enabled {
            continue;
        }
        let chain_name = &chain_config.name;
        let chain_id = chain_config.chain_id;
        let client = ctx.evm_provider(chain_id).await?;
        // Time lag offset tip.
        let block_confirmations = chain_config.block_confirmations;
        let timelag_client =
            Arc::new(TimeLag::new(client.clone(), block_confirmations));
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );

        for contract in &chain_config.contracts {
            match contract {
                Contract::VAnchor(config) => {
                    start_vanchor_events_watcher(
                        ctx,
                        config,
                        chain_id,
                        timelag_client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::SignatureBridge(config) => {
                    start_signature_bridge_events_watcher(
                        ctx,
                        config,
                        timelag_client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::MaspVanchor(_) => todo!(),
            }
        }
        // start the transaction queue after starting other tasks.
        start_tx_queue(ctx.clone(), chain_config.chain_id, store.clone())?;
    }
    Ok(())
}

/// Starts the event watcher for EVM VAnchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - VAnchor contract configuration
/// * `client` - EVM Chain api client
/// * `store` -[Sled](https://sled.rs)-based database store
async fn start_vanchor_events_watcher(
    ctx: &RelayerContext,
    config: &VAnchorContractConfig,
    chain_id: u32,
    client: Arc<TimeLagClient>,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "VAnchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = VAnchorContractWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "VAnchor events watcher for ({}) Started.",
            contract_address,
        );
        let contract_watcher = VAnchorContractWatcher::default();
        let proposal_signing_backend = make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            TypedChainId::Evm(chain_id),
            my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        tracing::debug!(
            %chain_id,
            %contract_address,
            "Fetching the Zero Hash from the contract",
        );
        let zero_hash = wrapper.contract.get_zero_hash(0).call().await?;
        tracing::debug!(
            %chain_id,
            %contract_address,
            %zero_hash,
            "Found the Zero Hash",
        );
        let mut zero_hash_bytes = [0u8; 32];
        zero_hash.to_big_endian(&mut zero_hash_bytes);

        let proposals_queue = queue::mem::InMemoryProposalsQueue::new();
        let time_delay_policy = {
            let defaults = SmartAnchorUpdatesConfig::default();
            let v = &my_config.smart_anchor_updates;
            let initial_delay = v
                .initial_time_delay
                .or(defaults.initial_time_delay)
                .expect("initial time delay is set by default");
            let min_delay = v
                .min_time_delay
                .or(defaults.min_time_delay)
                .expect("min time delay is set by default");
            let max_delay = v
                .max_time_delay
                .or(defaults.max_time_delay)
                .expect("max time delay is set by default");
            let window_size = v
                .time_delay_window_size
                .or(defaults.time_delay_window_size)
                .expect("time delay window size is set by default");

            policy::TimeDelayPolicy::builder()
                .initial_delay(initial_delay)
                .min_delay(min_delay)
                .max_delay(max_delay)
                .window_size(window_size)
                .build()
        };

        if my_config.smart_anchor_updates.enabled {
            tracing::info!(
                %chain_id,
                %contract_address,
                "Smart Anchor Updates enabled",
            );
        } else {
            tracing::info!(
                chain_id,
                %contract_address,
                "Smart Anchor Updates disabled",
            );
        }

        let enqueue_policy = my_config.smart_anchor_updates.enabled.then_some(
            (policy::AlwaysHigherNoncePolicy, time_delay_policy.clone()),
        );
        let dequeue_policy = my_config
            .smart_anchor_updates
            .enabled
            .then_some(time_delay_policy);

        let metrics = my_ctx.metrics.clone();
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let deposit_handler = VAnchorDepositHandler::builder()
                    .chain_id(chain_id)
                    .store(store.clone())
                    .proposals_queue(proposals_queue.clone())
                    .policy(enqueue_policy)
                    .build();
                let leaves_handler = VAnchorLeavesHandler::new(
                    chain_id.into(),
                    contract_address,
                    store.clone(),
                    zero_hash_bytes.to_vec(),
                )?;
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::new(chain_id.into());
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
                );

                let proposals_queue_task = queue::run(
                    proposals_queue,
                    dequeue_policy,
                    backend,
                    metrics,
                );

                tokio::select! {
                    _ = proposals_queue_task => {
                        tracing::warn!(
                            "Proposals queue task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let deposit_handler = VAnchorDepositHandler::builder()
                    .chain_id(chain_id)
                    .store(store.clone())
                    .proposals_queue(proposals_queue.clone())
                    .policy(enqueue_policy)
                    .build();
                let leaves_handler = VAnchorLeavesHandler::new(
                    chain_id.into(),
                    contract_address,
                    store.clone(),
                    zero_hash_bytes.to_vec(),
                )?;
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::new(chain_id.into());
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
                );

                let proposals_queue_task = queue::run(
                    proposals_queue,
                    dequeue_policy,
                    backend,
                    metrics,
                );

                tokio::select! {
                    _ = proposals_queue_task => {
                        tracing::warn!(
                            "Proposals queue task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = VAnchorLeavesHandler::new(
                    chain_id.into(),
                    contract_address,
                    store.clone(),
                    zero_hash_bytes.to_vec(),
                )?;
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::new(chain_id.into());
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
        };

        crate::Result::Ok(())
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for Signature Bridge contract.
pub async fn start_signature_bridge_events_watcher(
    ctx: &RelayerContext,
    config: &SignatureBridgeContractConfig,
    client: Arc<TimeLagClient>,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Signature Bridge events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;

    let wrapper =
        SignatureBridgeContractWrapper::new(config.clone(), client.clone());
    let metrics = ctx.metrics.clone();
    let my_ctx = ctx.clone();
    let task = async move {
        tracing::debug!(
            "Signature Bridge watcher for ({}) Started.",
            contract_address
        );
        let bridge_contract_watcher = SignatureBridgeContractWatcher::default();
        let governance_transfer_handler =
            SignatureBridgeGovernanceOwnershipTransferredHandler::default();
        let events_watcher_task = EventWatcher::run(
            &bridge_contract_watcher,
            client.clone(),
            store.clone(),
            wrapper.clone(),
            vec![Box::new(governance_transfer_handler)],
            &my_ctx,
        );
        let cmd_handler_task = BridgeWatcher::run(
            &bridge_contract_watcher,
            client,
            store,
            wrapper,
            metrics.clone(),
        );
        tokio::select! {
            _ = events_watcher_task => {
                tracing::warn!(
                    "signature bridge events watcher task stopped for ({})",
                    contract_address
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "signature bridge cmd handler task stopped for ({})",
                    contract_address
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Signature Bridge watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the transaction queue task
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `chain_name` - Name of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_tx_queue(
    ctx: RelayerContext,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    // Start tx_queue only when governance relaying or private tx relaying is enabled for relayer.
    if !ctx.config.features.governance_relay
        && !ctx.config.features.private_tx_relay
    {
        tracing::warn!("Tx Queue disabled for ({})", chain_id,);
        return Ok(());
    }

    let mut shutdown_signal = ctx.shutdown_signal();
    let tx_queue = TxQueue::new(ctx, chain_id.into(), store);

    tracing::debug!("Transaction Queue for ({}) Started.", chain_id);
    let task = async move {
        tokio::select! {
            _ = tx_queue.run() => {
                tracing::warn!(
                    "Transaction Queue task stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for ({})",
                    chain_id,
                );
            },
        }
    };
    // kick off the tx_queue.
    tokio::task::spawn(task);
    Ok(())
}
