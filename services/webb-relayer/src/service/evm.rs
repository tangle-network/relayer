use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use webb_bridge_registry_backends::dkg::DkgBridgeRegistryBackend;
use webb_bridge_registry_backends::mocked::MockedBridgeRegistryBackend;
use webb_event_watcher_traits::{BridgeWatcher, EthersClient, EventWatcher};
use webb_ew_evm::open_vanchor::{
    OpenVAnchorDepositHandler, OpenVAnchorLeavesHandler,
};
use webb_ew_evm::signature_bridge_watcher::{
    SignatureBridgeContractWatcher, SignatureBridgeContractWrapper,
    SignatureBridgeGovernanceOwnershipTransferredHandler,
};
use webb_ew_evm::vanchor::{
    VAnchorDepositHandler, VAnchorEncryptedOutputHandler, VAnchorLeavesHandler,
};
use webb_ew_evm::{
    OpenVAnchorContractWatcher, OpenVAnchorContractWrapper,
    VAnchorContractWatcher, VAnchorContractWrapper,
};
use webb_proposals::TypedChainId;
use webb_relayer_config::evm::{
    Contract, SignatureBridgeContractConfig, VAnchorContractConfig,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handlers::handle_evm_fee_info;
use webb_relayer_handlers::routes::{encrypted_outputs, leaves, metric};
use webb_relayer_tx_queue::evm::TxQueue;

use super::make_proposal_signing_backend;
use super::ProposalSigningBackendSelector;

/// Type alias for providers
pub type Client = EthersClient;

/// Setup and build all the EVM web services and handlers.
pub fn build_web_services() -> Router<Arc<RelayerContext>> {
    Router::new()
        .route(
            "/leaves/evm/:chain_id/:contract",
            get(leaves::handle_leaves_cache_evm),
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
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::OpenVAnchor(config) => {
                    start_open_vanchor_events_watcher(
                        ctx,
                        config,
                        chain_id,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::SignatureBridge(config) => {
                    start_signature_bridge_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
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
    client: Arc<Client>,
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
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let bridge_registry =
                    DkgBridgeRegistryBackend::new(backend.client.clone());
                let deposit_handler = VAnchorDepositHandler::new(
                    chain_id.into(),
                    store.clone(),
                    backend,
                    bridge_registry,
                );
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
            ProposalSigningBackendSelector::Mocked(backend) => {
                let bridge_registry =
                    MockedBridgeRegistryBackend::builder().build();
                let deposit_handler = VAnchorDepositHandler::new(
                    chain_id.into(),
                    store.clone(),
                    backend,
                    bridge_registry,
                );
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

/// Starts the event watcher for EVM OpenVAnchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - VAnchor contract configuration
/// * `client` - EVM Chain api client
/// * `store` -[Sled](https://sled.rs)-based database store
pub async fn start_open_vanchor_events_watcher(
    ctx: &RelayerContext,
    config: &VAnchorContractConfig,
    chain_id: u32,
    client: Arc<Client>,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Open VAnchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = OpenVAnchorContractWrapper::new(
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
            "Open VAnchor events watcher for ({}) Started.",
            contract_address,
        );
        let contract_watcher = OpenVAnchorContractWatcher::default();
        let proposal_signing_backend = make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            webb_proposals::TypedChainId::Evm(chain_id),
            my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let bridge_registry =
                    DkgBridgeRegistryBackend::new(backend.client.clone());
                let deposit_handler =
                    OpenVAnchorDepositHandler::new(backend, bridge_registry);
                let leaves_handler = OpenVAnchorLeavesHandler::default();

                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let bridge_registry =
                    MockedBridgeRegistryBackend::builder().build();
                let deposit_handler =
                    OpenVAnchorDepositHandler::new(backend, bridge_registry);
                let leaves_handler = OpenVAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = OpenVAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
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
    client: Arc<Client>,
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
    // Start tx_queue only when governance relaying feature is enabled for relayer.
    if !ctx.config.features.governance_relay {
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
