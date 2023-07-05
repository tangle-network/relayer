use std::sync::Arc;
use webb::substrate::subxt::{self, PolkadotConfig};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_ew_dkg::{
    DKGMetadataWatcher, DKGProposalHandlerWatcher, DKGPublicKeyChangedHandler,
    ProposalSignedHandler,
};
use webb_relayer_config::substrate::{
    DKGPalletConfig, DKGProposalHandlerPalletConfig, Pallet, SubstrateConfig,
};
use webb_relayer_context::RelayerContext;

/// Type alias for the Tangle DefaultConfig
pub type TangleClient = subxt::OnlineClient<PolkadotConfig>;

/// Fires up all background services for all Substrate chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
pub async fn ignite(
    ctx: RelayerContext,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    for (_, node_config) in ctx.clone().config.substrate {
        if !node_config.enabled {
            continue;
        }
        ignite_tangle_runtime(ctx.clone(), store.clone(), &node_config).await?;
    }
    Ok(())
}

async fn ignite_tangle_runtime(
    ctx: RelayerContext,
    store: Arc<super::Store>,
    node_config: &SubstrateConfig,
) -> crate::Result<()> {
    let chain_id = node_config.chain_id;
    for pallet in &node_config.pallets {
        match pallet {
            Pallet::DKGProposalHandler(config) => {
                start_dkg_proposal_handler(
                    ctx.clone(),
                    config,
                    chain_id,
                    store.clone(),
                )?;
            }
            Pallet::Dkg(config) => {
                start_dkg_pallet_watcher(
                    ctx.clone(),
                    config,
                    chain_id,
                    store.clone(),
                )?;
            }
            Pallet::DKGProposals(_) => {
                // TODO(@shekohex): start the dkg proposals service
                unimplemented!()
            }
        }
    }
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
    ctx: RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
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
            ctx.clone(),
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
    ctx: RelayerContext,
    config: &DKGPalletConfig,
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
            ctx.clone(),
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
