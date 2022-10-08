use std::sync::Arc;

use self::traits::{BlockEventHandler, BlockWatcher};
use crate::config::*;
use crate::context::RelayerContext;
use crate::service::{Client, Store};
use crate::store::SledStore;
use crate::types::rpc_url::RpcUrl;
use beacon_chain_relay::beacon_rpc_client::BeaconRPCClient;
use ethereum_types::U256;
use webb::evm::ethers::types::{Block, TxHash};

mod traits;

#[derive(Copy, Clone, Debug, Default)]
pub struct BlockHeaderWatcher;

#[async_trait::async_trait]
impl BlockWatcher for BlockHeaderWatcher {
    const TAG: &'static str = "Block Watcher";
    type Store = SledStore;
}

struct BlockFinalityHandler {
    light_client_rpc_url: RpcUrl,
}

#[async_trait::async_trait]
impl BlockEventHandler for BlockFinalityHandler {
    type Store = SledStore;

    async fn handle_block(
        &self,
        store: Arc<Self::Store>,
        block: Block<TxHash>,
    ) -> crate::Result<()> {
        tracing::debug!(?block);
        const TIMEOUT_SECONDS: u64 = 30;
        const TIMEOUT_STATE_SECONDS: u64 = 1000;

        if let Some(n) = block.number {
            let url = self.light_client_rpc_url.join(&n.to_string()).unwrap();
            let beacon_rpc_client = BeaconRPCClient::new(
                &url.to_string(),
                TIMEOUT_SECONDS,
                TIMEOUT_STATE_SECONDS,
            );
            let rpc_json_str =
                beacon_rpc_client.get_json_from_raw_request(&url.to_string());
            tracing::debug!("{:?}", rpc_json_str);
        }

        // TODO: Do something with the RPC JSON string
        // TODO: Connect to parachain and submit data in an extrinsics
        Ok(())
    }
}

pub async fn start_block_relay_service(
    ctx: &RelayerContext,
    chain_id: U256,
    client: Arc<Client>,
    store: Arc<Store>,
    light_client_rpc_url: RpcUrl,
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let my_ctx = ctx.clone();
    let task = async move {
        tracing::debug!(
            "Block header watcher started for ({}) Started.",
            chain_id,
        );

        let block_watcher = BlockHeaderWatcher::default();
        let block_finality_handler = BlockFinalityHandler {
            light_client_rpc_url,
        };
        let block_watcher_task = block_watcher.run(
            client,
            store,
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
