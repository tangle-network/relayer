use std::sync::Arc;

use crate::light_client::LightClientPoller;
use ethereum_types::U256;
use webb_relayer::service::{Client, Store};
use webb_relayer_config::block_poller::BlockPollerConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_store::SledStore;
use webb_relayer_utils::Result;

mod light_client;

/// A struct for listening to blocks / block headers that implements
/// the [`LightClientPoller`] trait.
#[derive(Copy, Clone, Debug, Default)]
pub struct LightClientWatcher;

#[async_trait::async_trait]
impl LightClientPoller for LightClientWatcher {
    const TAG: &'static str = "Block Watcher";
    type Store = SledStore;
}

/// Start the block poller service which polls ETH blocks
pub fn start_light_client_service(
    ctx: &RelayerContext,
    chain_id: U256,
    client: Arc<Client>,
    store: Arc<Store>,
    poller_config: BlockPollerConfig,
) -> Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let _my_ctx = ctx.clone();
    tracing::info!("Starting block relay service");
    let task = async move {
        tracing::debug!(
            "Block header watcher started for ({}) Started.",
            chain_id,
        );

        let light_client_watcher = LightClientWatcher::default();
        let light_client_watcher_task =
            light_client_watcher.run(client, store, poller_config);
        tokio::select! {
            _ = light_client_watcher_task => {
                tracing::warn!("Block watcher stopped unexpectedly for chain {}", chain_id);
            },
            _ = shutdown_signal.recv() => {
                tracing::debug!("Shutting down the network for {}", chain_id);
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}
