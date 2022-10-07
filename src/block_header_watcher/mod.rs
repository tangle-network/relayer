use std::sync::Arc;

use crate::config::*;
use crate::context::RelayerContext;
use crate::service::{Client, Store};
use crate::store::SledStore;
use ethereum_types::U256;
use webb::evm::ethers::types::{Block, TxHash};

use self::traits::{BlockEventHandler, BlockWatcher};

mod traits;

#[derive(Copy, Clone, Debug, Default)]
pub struct BlockHeaderWatcher;

#[async_trait::async_trait]
impl BlockWatcher for BlockHeaderWatcher {
    const TAG: &'static str = "Block Watcher";
    type Store = SledStore;
}

struct BlockFinalityFetcher;

#[async_trait::async_trait]
impl BlockEventHandler for BlockFinalityFetcher {
    type Store = SledStore;

    async fn handle_block(
        &self,
        store: Arc<Self::Store>,
        block: Block<TxHash>,
    ) -> crate::Result<()> {
        // let url = config.light_client_rpc_url.join(block.number).unwrap();
        // let beacon_rpc_client = BeaconRpcClient::new(url);
        // let rpc_json_str = beacon_rpc_client.get_json_from_raw_request(url);

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
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let my_ctx = ctx.clone();
    let task = async move {
        tracing::debug!(
            "Block header watcher started for ({}) Started.",
            chain_id,
        );

        let block_watcher = BlockHeaderWatcher::default();
        let block_finality_handler = BlockFinalityFetcher {};
        let block_watcher_task = block_watcher.run(
            client,
            store,
            vec![Box::new(block_finality_handler)],
        );
        tokio::select! {
            _ = block_watcher_task => {
            },
            _ = shutdown_signal.recv() => {
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}
