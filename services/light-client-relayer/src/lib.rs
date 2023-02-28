use crate::light_client::LightClientPoller;

use eth2_to_substrate_relay::config::Config;
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
    config: Config,
) -> Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let _my_ctx = ctx.clone();
    let chain_id = config.chain_id;
    tracing::info!("Starting block relay service");
    let task = async move {
        tracing::debug!(
            "Block header watcher started for ({}) Started.",
            chain_id,
        );

        let light_client_watcher = LightClientWatcher::default();
        /*let light_client_watcher_task =
        light_client_watcher.run(client, store, poller_config);*/
        //let config_for_tests = get_test_config();
        let light_client_watcher_task = light_client_watcher.run(config);
        tokio::select! {
            res = light_client_watcher_task => {
                tracing::warn!("Block watcher stopped unexpectedly for chain {} | reason: {:?}", chain_id, res);
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
