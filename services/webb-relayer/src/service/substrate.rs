use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use sp_core::sr25519;
use webb::substrate::subxt::config::ExtrinsicParams;
use webb::substrate::subxt::{self, PolkadotConfig};
use webb_bridge_registry_backends::dkg::DkgBridgeRegistryBackend;
use webb_bridge_registry_backends::mocked::MockedBridgeRegistryBackend;
use webb_event_watcher_traits::{
    SubstrateBridgeWatcher, SubstrateEventWatcher,
};
use webb_ew_dkg::{
    DKGMetadataWatcher, DKGProposalHandlerWatcher, DKGPublicKeyChangedHandler,
    ProposalSignedHandler,
};
use webb_ew_substrate::{
    MaintainerSetEventHandler, SubstrateBridgeEventWatcher,
    SubstrateVAnchorDepositHandler, SubstrateVAnchorEncryptedOutputHandler,
    SubstrateVAnchorEventWatcher, SubstrateVAnchorLeavesHandler,
};
use webb_relayer_config::substrate::{
    DKGPalletConfig, DKGProposalHandlerPalletConfig, Pallet,
    SignatureBridgePalletConfig, SubstrateConfig, VAnchorBn254PalletConfig,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handlers::routes::{leaves, metric};
use webb_relayer_tx_queue::substrate::SubstrateTxQueue;

use super::ProposalSigningBackendSelector;

/// Type alias for the Tangle DefaultConfig
pub type TangleClient = subxt::OnlineClient<PolkadotConfig>;

/// Setup and build all the Substrate web services and handlers.
pub fn build_web_services() -> Router<Arc<RelayerContext>> {
    Router::new()
        .route(
            "/leaves/substrate/:chain_id/:tree_id/:pallet_id",
            get(leaves::handle_leaves_cache_substrate),
        )
        .route(
            "/metrics/substrate/:chain_id/:tree_id/:pallet_id",
            get(metric::handle_substrate_metric_info),
        )
}

/// Fires up all background services for all Substrate chains configured in the config file.
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
    for (node_name, node_config) in &ctx.config.substrate {
        if !node_config.enabled {
            continue;
        }
        ignite_tangle_runtime(ctx, store.clone(), node_name, node_config)
            .await?;
    }
    Ok(())
}

async fn ignite_tangle_runtime(
    ctx: &RelayerContext,
    store: Arc<super::Store>,
    node_name: &str,
    node_config: &SubstrateConfig,
) -> crate::Result<()> {
    let client = ctx.substrate_provider::<PolkadotConfig>(node_name).await?;
    let chain_id = node_config.chain_id;
    for pallet in &node_config.pallets {
        match pallet {
            Pallet::DKGProposalHandler(config) => {
                start_dkg_proposal_handler(
                    ctx,
                    config,
                    client.clone(),
                    chain_id,
                    store.clone(),
                )?;
            }
            Pallet::Dkg(config) => {
                start_dkg_pallet_watcher(
                    ctx,
                    config,
                    client.clone(),
                    chain_id,
                    store.clone(),
                )?;
            }
            Pallet::DKGProposals(_) => {
                // TODO(@shekohex): start the dkg proposals service
            }
            Pallet::SignatureBridge(config) => {
                start_substrate_signature_bridge_events_watcher(
                    ctx.clone(),
                    config,
                    client.clone(),
                    chain_id,
                    store.clone(),
                )
                .await?;
            }
            Pallet::VAnchorBn254(config) => {
                start_substrate_vanchor_event_watcher(
                    ctx,
                    config,
                    client.clone(),
                    chain_id,
                    store.clone(),
                )?;
            }
        }
    }
    // start the transaction queue for dkg-substrate extrinsics after starting other tasks.
    start_tx_queue::<PolkadotConfig>(ctx.clone(), chain_id, store.clone())?;
    Ok(())
}

/// Starts the event watcher for DKG proposal handler events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - DKG proposal handler configuration
/// * `client` - DKG client
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_proposal_handler(
    ctx: &RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
    client: TangleClient,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Proposal Handler events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!(
        "DKG Proposal Handler events watcher for ({}) Started.",
        chain_id,
    );
    let mut shutdown_signal = ctx.shutdown_signal();
    let metrics = ctx.metrics.clone();
    let my_config = config.clone();
    let task = async move {
        let proposal_handler_watcher = DKGProposalHandlerWatcher::default();
        let proposal_signed_handler = ProposalSignedHandler::default();
        let proposal_handler_watcher_task = proposal_handler_watcher.run(
            chain_id,
            client.into(),
            store,
            my_config.events_watcher,
            vec![Box::new(proposal_signed_handler)],
            metrics,
        );
        tokio::select! {
            _ = proposal_handler_watcher_task => {
                tracing::warn!(
                    "DKG Proposal Handler events watcher stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Proposal Handler events watcher for ({})",
                    chain_id,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for DKG pallet events watcher.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - DKG pallet configuration
/// * `client` - DKG client
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_pallet_watcher(
    ctx: &RelayerContext,
    config: &DKGPalletConfig,
    client: TangleClient,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this pallet.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Pallet events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!("DKG Pallet events watcher for ({}) Started.", chain_id,);
    let mut shutdown_signal = ctx.shutdown_signal();
    let webb_config = ctx.config.clone();
    let metrics = ctx.metrics.clone();
    let my_config = config.clone();
    let task = async move {
        let dkg_event_watcher = DKGMetadataWatcher::default();
        let public_key_changed_handler =
            DKGPublicKeyChangedHandler::new(webb_config);

        let dkg_event_watcher_task = dkg_event_watcher.run(
            chain_id,
            client.into(),
            store,
            my_config.events_watcher,
            vec![Box::new(public_key_changed_handler)],
            metrics,
        );
        tokio::select! {
            _ = dkg_event_watcher_task => {
                tracing::warn!(
                    "DKG Pallet events watcher stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Pallet events watcher for ({})",
                    chain_id,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for Substrate vanchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - VAnchorBn254 configuration
/// * `client` - WebbProtocol client
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_substrate_vanchor_event_watcher(
    ctx: &RelayerContext,
    config: &VAnchorBn254PalletConfig,
    client: TangleClient,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate VAnchor events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate VAnchor events watcher for ({}) Started.",
        chain_id,
    );
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let metrics = ctx.metrics.clone();
    let task = async move {
        let proposal_signing_backend = super::make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            webb_proposals::TypedChainId::Substrate(chain_id),
            my_config.linked_anchors.clone(),
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let bridge_registry =
                    DkgBridgeRegistryBackend::new(backend.client.clone());

                let deposit_handler = SubstrateVAnchorDepositHandler::new(
                    backend,
                    bridge_registry,
                    my_config.linked_anchors,
                );
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );

                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (DKG Backend) task stopped for ({})",
                            chain_id,
                        );
                    },

                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (DKG Backend) for ({})",
                            chain_id,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let bridge_registry =
                    MockedBridgeRegistryBackend::builder().build();

                let deposit_handler = SubstrateVAnchorDepositHandler::new(
                    backend,
                    bridge_registry,
                    my_config.linked_anchors,
                );
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (Mocked Backend) task stopped for ({})",
                            chain_id,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (Mocked Backend) for ({})",
                            chain_id,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher task stopped for ({})",
                            chain_id,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher task for ({})",
                            chain_id,
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

/// Starts the event watcher for Signature Bridge Pallet.
pub async fn start_substrate_signature_bridge_events_watcher(
    ctx: RelayerContext,
    config: &SignatureBridgePalletConfig,
    client: TangleClient,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate Signature Bridge events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    let mut shutdown_signal = ctx.shutdown_signal();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "Substrate Signature Bridge watcher for ({}) Started.",
            chain_id
        );
        let substrate_bridge_watcher = SubstrateBridgeEventWatcher::default();
        let bridge_event_handler = MaintainerSetEventHandler::default();
        let events_watcher_task = SubstrateEventWatcher::run(
            &substrate_bridge_watcher,
            chain_id,
            client.clone().into(),
            store.clone(),
            my_config.events_watcher,
            vec![Box::new(bridge_event_handler)],
            ctx.metrics.clone(),
        );
        let cmd_handler_task = SubstrateBridgeWatcher::run(
            &substrate_bridge_watcher,
            chain_id,
            client.into(),
            store.clone(),
        );
        tokio::select! {
            _ = events_watcher_task => {
                tracing::warn!(
                    "Substrate signature bridge events watcher task stopped for ({})",
                    chain_id
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "Substrate signature bridge cmd handler task stopped for ({})",
                    chain_id
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Substrate Signature Bridge watcher for ({})",
                    chain_id,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the transaction queue task for Substrate extrinsics
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `chain_name` - Name of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_tx_queue<X>(
    ctx: RelayerContext,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()>
where
    X: subxt::Config + Send + Sync,
    <<X>::ExtrinsicParams as ExtrinsicParams<
        <X>::Index,
        <X>::Hash,
    >>::OtherParams: Default + Send + Sync,
    <X>::Signature: From<sr25519::Signature>,
    <X>::Address: From<<X>::AccountId>,
    <X as subxt::Config>::AccountId:
        From<sp_runtime::AccountId32> + Send + Sync,
{
    let mut shutdown_signal = ctx.shutdown_signal();

    let tx_queue = SubstrateTxQueue::new(ctx, chain_id, store);

    tracing::debug!("Transaction Queue for node({}) Started.", chain_id);
    let task = async move {
        tokio::select! {
            _ = tx_queue.run::<X>() => {
                tracing::warn!(
                    "Transaction Queue task stopped for node({})",
                    chain_id
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for node({})",
                    chain_id
                );
            },
        }
    };
    // kick off the substrate tx_queue.
    tokio::task::spawn(task);
    Ok(())
}
