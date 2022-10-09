use std::sync::Arc;

use self::block_poller::{BlockEventHandler, BlockPoller};
use crate::config::*;
use crate::context::RelayerContext;
use crate::service::{Client, Store};
use crate::store::SledStore;
use crate::types::rpc_url::RpcUrl;
use beacon_chain_relay::beacon_rpc_client::BeaconRPCClient;
use ethereum_types::U256;
use webb::evm::ethers::types::{Block, TxHash};

mod block_poller;

/// A struct for listening to blocks / block headers that implements
/// the [`BlockPoller`] trait.
#[derive(Copy, Clone, Debug, Default)]
pub struct BlockWatcher;

#[async_trait::async_trait]
impl BlockPoller for BlockWatcher {
    const TAG: &'static str = "Block Watcher";
    type Store = SledStore;
}

#[derive(Clone, Debug)]
struct BlockFinalityHandler {
    light_client_rpc_url: Option<RpcUrl>,
}

#[async_trait::async_trait]
impl BlockEventHandler for BlockFinalityHandler {
    type Store = SledStore;

    async fn handle_block(
        &self,
        store: Arc<Self::Store>,
        block: Block<TxHash>,
    ) -> crate::Result<()> {
        tracing::debug!("{}", serde_json::to_string_pretty(&block)?);
        const TIMEOUT_SECONDS: u64 = 30;
        const TIMEOUT_STATE_SECONDS: u64 = 1000;

        if let Some(light_client_rpc_url) = &self.light_client_rpc_url {
            if let Some(n) = block.number {
                let url = format!(
                    "{}/eth/v2/beacon/blocks/{}",
                    light_client_rpc_url, config.first_slot
                );
                tracing::trace!("url: {}", url);
                let beacon_rpc_client = BeaconRPCClient::new(
                    &url.to_string(),
                    TIMEOUT_SECONDS,
                    TIMEOUT_STATE_SECONDS,
                );
                let rpc_json_str = beacon_rpc_client
                    .get_json_from_raw_request(&url.to_string());
                tracing::debug!("{:?}", rpc_json_str);
            }
        }

        // TODO: Do something with the RPC JSON string
        // TODO: Connect to parachain and submit data in an extrinsics
        Ok(())
    }
}

/// Start the block relay service which watches ETH2 chains for blocks
pub fn start_block_relay_service(
    ctx: &RelayerContext,
    chain_id: U256,
    client: Arc<Client>,
    store: Arc<Store>,
    listener_config: BlockListenerConfig,
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let _my_ctx = ctx.clone();
    tracing::info!("Starting block relay service");
    let task = async move {
        tracing::debug!(
            "Block header watcher started for ({}) Started.",
            chain_id,
        );

        let block_watcher = BlockWatcher::default();
        let block_finality_handler = BlockFinalityHandler {
            light_client_rpc_url: listener_config.light_client_rpc_url.clone(),
        };
        let block_watcher_task = block_watcher.run(
            client,
            store,
            listener_config,
            vec![Box::new(block_finality_handler)],
        );
        tokio::select! {
            _ = block_watcher_task => {
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
