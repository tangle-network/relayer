use std::sync::Arc;
use webb::substrate::subxt;
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_ew_dkg::*;
use webb_relayer_config::substrate::{
    JobsPalletConfig, Pallet, SubstrateConfig,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::TangleRuntimeConfig;

use webb_relayer_tx_queue::substrate::SubstrateTxQueue;

/// Type alias for the Tangle DefaultConfig
pub type TangleClient = subxt::OnlineClient<TangleRuntimeConfig>;

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
            Pallet::Jobs(config) => {
                start_job_result_watcher(
                    ctx.clone(),
                    config,
                    chain_id,
                    store.clone(),
                )?;
            }
        }
    }
    // start the transaction queue for dkg-substrate extrinsics after starting other tasks.
    start_tx_queue::<TangleRuntimeConfig>(ctx, chain_id, store)?;
    Ok(())
}

/// Starts the event watcher for JobResultSubmitted events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - Jobs Result handler configuration
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_job_result_watcher(
    ctx: RelayerContext,
    config: &JobsPalletConfig,
    chain_id: u32,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Job Result events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!("Job Result events watcher for ({}) Started.", chain_id,);
    let mut shutdown_signal = ctx.shutdown_signal();
    let metrics = ctx.metrics.clone();
    let webb_config = ctx.config.clone();
    let my_config = config.clone();
    let task = async move {
        let job_result_watcher = JobResultWatcher::default();
        let job_result_event_handler = JobResultHandler::new(webb_config);
        let job_result_watcher_task = job_result_watcher.run(
            chain_id,
            ctx.clone(),
            store,
            my_config.events_watcher,
            vec![Box::new(job_result_event_handler)],
            metrics,
        );
        tokio::select! {
            _ = job_result_watcher_task => {
                tracing::warn!(
                    "Job Result events watcher stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Job Result events watcher for ({})",
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
