use std::sync::Arc;

use crate::block_poller::{BlockPoller, BlockPollingHandler};
use ethereum_types::U256;
use webb::evm::ethers::types::{Block, TxHash};
use webb_relayer::context::RelayerContext;
use webb_relayer::service::{Client, Store};
use webb_relayer_config::block_poller::BlockPollerConfig;
use webb_relayer_store::SledStore;
use webb_relayer_utils::{Error, Result};

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
struct BlockListener;

#[async_trait::async_trait]
impl BlockPollingHandler for BlockListener {
    type Store = SledStore;

    async fn handle_block(
        &self,
        _store: Arc<Self::Store>,
        block: Block<TxHash>,
    ) -> Result<()> {
        tracing::debug!("{}", serde_json::to_string_pretty(&block)?);
        Ok(())
    }
}

/// Start the block poller service which polls ETH blocks
pub fn start_block_poller_service(
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

        let block_watcher = BlockWatcher::default();
        let block_finality_handler = BlockListener;
        let block_watcher_task = block_watcher.run(
            client,
            store,
            poller_config,
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

/// Starts all background services for all chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
///
/// # Examples
///
/// ```
/// let _ = service::ignite(&ctx, Arc::new(store)).await?;
/// ```
pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<Store>,
) -> crate::Result<()> {
    tracing::debug!(
        "Relayer configuration: {}",
        serde_json::to_string_pretty(&ctx.config)?
    );

    // now we go through each chain, in our configuration
    for chain_config in ctx.config.evm.values() {
        if !chain_config.enabled {
            continue;
        }
        let chain_name = &chain_config.name;
        let chain_id = U256::from(chain_config.chain_id);
        let provider = ctx.evm_provider(&chain_id.to_string()).await?;
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );

        if let Some(poller_config) = &chain_config.block_poller {
            tracing::debug!(
                "Starting block relay ({})",
                poller_config,
            );
            start_block_poller_service(
                ctx,
                chain_id,
                client,
                store.clone(),
                poller_config.clone(),
            )?;
        }
    }
    Ok(())
}
