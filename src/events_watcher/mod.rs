use std::cmp;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use futures::prelude::*;
use webb::evm::ethers::providers::Middleware;
use webb::evm::ethers::{contract, providers, types};

use crate::store::{HistoryStore, ProposalStore, TxQueueStore};

mod tornado_leaves_watcher;
pub use tornado_leaves_watcher::*;

mod anchor_watcher;
pub use anchor_watcher::*;

mod bridge_watcher;
pub use bridge_watcher::*;

mod anchor_watcher_over_dkg;
pub use anchor_watcher_over_dkg::*;

/// A watchable contract is a contract used in the [EventWatcher]
pub trait WatchableContract: Send + Sync {
    /// The block number where this contract is deployed.
    fn deployed_at(&self) -> types::U64;

    /// How often this contract should be polled for events.
    fn polling_interval(&self) -> Duration;
}

#[async_trait::async_trait]
pub trait EventWatcher {
    const TAG: &'static str;
    type Middleware: providers::Middleware + 'static;
    type Contract: Deref<Target = contract::Contract<Self::Middleware>>
        + WatchableContract;
    type Events: contract::EthLogDecode;
    type Store: HistoryStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, contract::LogMeta),
    ) -> anyhow::Result<()>;

    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = %client.get_chainid().await?,
            address = %contract.address(),
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<Self::Middleware>,
        store: Arc<Self::Store>,
        contract: Self::Contract,
    ) -> anyhow::Result<()> {
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async {
            let step = types::U64::from(50);
            // saves the last time we printed sync progress.
            let mut instant = std::time::Instant::now();
            let chain_id =
                client.get_chainid().map_err(anyhow::Error::from).await?;
            // now we start polling for new events.
            loop {
                let block = store.get_last_block_number(
                    (chain_id, contract.address()),
                    contract.deployed_at(),
                )?;
                let current_block_number = client
                    .get_block_number()
                    .map_err(anyhow::Error::from)
                    .await?;
                tracing::trace!(
                    "Latest block number: #{}",
                    current_block_number
                );
                let dest_block = cmp::min(block + step, current_block_number);
                // check if we are now on the latest block.
                let should_cooldown = dest_block == current_block_number;
                tracing::trace!("Reading from #{} to #{}", block, dest_block);
                // Only handle events from found blocks if they are new
                if dest_block != block {
                    let events_filter = contract
                        .event_with_filter::<Self::Events>(Default::default())
                        .from_block(block)
                        .to_block(dest_block);
                    let found_events = events_filter
                        .query_with_meta()
                        .map_err(anyhow::Error::from)
                        .await?;

                    tracing::trace!("Found #{} events", found_events.len());

                    for (event, log) in found_events {
                        let result = self
                            .handle_event(
                                store.clone(),
                                &contract,
                                (event, log.clone()),
                            )
                            .await;
                        match result {
                            Ok(_) => {
                                store.set_last_block_number(
                                    (chain_id, contract.address()),
                                    log.block_number,
                                )?;
                                tracing::trace!(
                                    "event handled successfully. at #{}",
                                    log.block_number
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error while handling event: {}",
                                    e
                                );
                                tracing::warn!("Restarting event watcher ...");
                                // this a transient error, so we will retry again.
                                return Err(backoff::Error::transient(e));
                            }
                        }
                    }

                    // move forward.
                    store.set_last_block_number(
                        (chain_id, contract.address()),
                        dest_block,
                    )?;
                }
                tracing::trace!("Polled from #{} to #{}", block, dest_block);
                if should_cooldown {
                    let duration = contract.polling_interval();
                    tracing::trace!(
                        "Cooldown a bit for {}ms",
                        duration.as_millis()
                    );
                    tokio::time::sleep(duration).await;
                }

                // only print the progress if 7 seconds is passed.
                if instant.elapsed() > Duration::from_secs(7) {
                    // calculate sync progress.
                    let total = current_block_number.as_u64() as f64;
                    let current_value = dest_block.as_u64() as f64;
                    let diff = total - current_value;
                    let percentage = (diff / current_value) * 100.0;
                    // should be always less that 100.
                    // and this would be our current progress.
                    let sync_progress = 100.0 - percentage;
                    tracing::info!(
                        "🔄 #{} of #{} ({:.4}%)",
                        dest_block,
                        current_block_number,
                        sync_progress
                    );
                    instant = std::time::Instant::now();
                }
            }
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait BridgeWatcher: EventWatcher
where
    Self::Store: TxQueueStore + ProposalStore,
{
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        cmd: BridgeCommand,
    ) -> anyhow::Result<()>;

    /// Returns a task that should be running in the background
    /// that will watch for all commands
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = %client.get_chainid().await?,
            address = %contract.address(),
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<Self::Middleware>,
        store: Arc<Self::Store>,
        contract: Self::Contract,
    ) -> anyhow::Result<()> {
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async {
            let my_address = contract.address();
            let my_chain_id =
                client.get_chainid().map_err(anyhow::Error::from).await?;
            let my_key = BridgeKey::new(my_address, my_chain_id);
            let mut rx = BridgeRegistry::register(my_key);
            while let Ok(command) = rx.recv().await {
                let result =
                    self.handle_cmd(store.clone(), &contract, command).await;
                match result {
                    Ok(_) => {
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Error while handle_cmd {}", e);
                        // this a transient error, so we will retry again.
                        // Internally it would use a queue so the value would be still in
                        // the queue.
                        tracing::warn!("Restarting bridge event watcher ...");
                        return Err(backoff::Error::transient(e));
                    }
                }
            }
            // loop ended, unregister.
            BridgeRegistry::unregister(my_key);
            // whenever this loop stops, we will restart the whole task again.
            // that way we never have to worry about closed channels.
            Err(backoff::Error::transient(anyhow::anyhow!("Restarting")))
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}
