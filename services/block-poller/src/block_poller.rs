use futures::prelude::*;
use std::cmp;
use std::sync::Arc;
use std::time::Duration;
use webb_relayer_config::block_poller::BlockPollerConfig;

use webb::evm::ethers::{
    providers::{self, Middleware, QuorumProvider},
    types::{Block, TxHash},
};

use webb_relayer_store::HistoryStore;
use webb_relayer_utils::retry;

/// A trait that defines a handler for a specific set of event types.
///
/// The handlers are implemented separately from the watchers, so that we can have
/// one event watcher and many event handlers that will run in parallel.
#[async_trait::async_trait]
pub trait BlockPollingHandler {
    /// The storage backend that this handler will use.
    type Store: HistoryStore;
    /// a method to be called with the event information,
    /// it is up to the handler to decide what to do with the event.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be discarded. to have a retry mechanism, use the [`BlockPollingHandlerWithRetry::handle_event_with_retry`] method
    /// which does exactly what it says.
    ///
    /// If this method returns Ok(true), the event will be marked as handled.
    async fn handle_block(
        &self,
        store: Arc<Self::Store>,
        block: Block<TxHash>,
    ) -> crate::Result<()>;
}

/// An Auxiliary trait to handle events with retry logic.
///
/// this trait is automatically implemented for all the event handlers.
#[async_trait::async_trait]
pub trait BlockPollingHandlerWithRetry: BlockPollingHandler {
    /// A method to be called with the event information,
    /// it is up to the handler to decide what to do with the block.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be retried again, depends on the retry strategy. if you do not care about the retry
    /// strategy, use the [`EventHandler::handle_event`] method instead.
    ///
    /// If this method returns Ok(true), the event will be marked as handled.
    ///
    /// **Note**: this method is automatically implemented for all the event handlers.
    async fn handle_block_with_retry(
        &self,
        store: Arc<Self::Store>,
        block: Block<TxHash>,
        backoff: impl backoff::backoff::Backoff + Send + Sync + 'static,
    ) -> crate::Result<()> {
        let wrapped_task = || {
            self.handle_block(store.clone(), block.clone())
                .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T> BlockPollingHandlerWithRetry for T where T: BlockPollingHandler + ?Sized {}

pub type BlockPollingHandlerFor<W> = Box<
    dyn BlockPollingHandler<Store = <W as BlockPoller>::Store> + Send + Sync,
>;

/// A trait for watching block headers using a provider.
/// BlockPoller trait exists for EVM based
#[async_trait::async_trait]
pub trait BlockPoller {
    /// A Helper tag used to identify the event watcher during the logs.
    const TAG: &'static str;
    /// The Storage backend that will be used to store the required state for this event watcher
    type Store: HistoryStore;
    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = ?client.get_chainid().await,
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<
            providers::Provider<
                providers::RetryClient<QuorumProvider<providers::Http>>,
            >,
        >,
        store: Arc<Self::Store>,
        listener_config: BlockPollerConfig,
        handlers: Vec<BlockPollingHandlerFor<Self>>,
    ) -> crate::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let task = || async {
            // Move one block at a time
            let step = listener_config.max_blocks_per_step;
            // saves the last time we printed sync progress.
            let chain_id = client
                .get_chainid()
                .map_err(Into::into)
                .map_err(backoff::Error::transient)
                .await?
                .as_u32();
            tracing::info!("chain id: {}", chain_id);
            // now we start polling for new events.
            loop {
                let block = store.get_last_block_number(
                    chain_id,
                    listener_config.start_block.unwrap_or_default(),
                )?;
                tracing::trace!(
                    "last block number: {}",
                    listener_config.start_block.unwrap_or_default()
                );
                let current_block_number_result: Result<
                    _,
                    backoff::Error<crate::Error>,
                > = client
                    .get_block_number()
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)
                    .await;

                let current_block_number = match current_block_number_result {
                    Ok(block_number) => block_number.as_u64(),
                    Err(e) => {
                        tracing::error!(
                            "Error {:?} while getting block number",
                            e
                        );
                        0u64
                    }
                };

                tracing::trace!(
                    "Latest block number: #{}",
                    current_block_number
                );
                let dest_block = cmp::min(block + step, current_block_number);
                let should_cooldown = dest_block == current_block_number;
                tracing::trace!("Reading from #{} to #{}", block, dest_block);
                // Only handle events from found blocks if they are new
                if dest_block != block {
                    match client.get_block(dest_block).await {
                        Ok(Some(block)) => {
                            // wraps each handler future in a retry logic, that will retry the handler
                            // if it fails, up to `MAX_RETRY_COUNT`, after this it will ignore that event for
                            // that specific handler.
                            const MAX_RETRY_COUNT: usize = 5;
                            let tasks = handlers.iter().map(|handler| {
                                // a constant backoff with maximum retry count is used here.
                                let backoff =
                                    retry::ConstantWithMaxRetryCount::new(
                                        Duration::from_millis(100),
                                        MAX_RETRY_COUNT,
                                    );
                                handler.handle_block_with_retry(
                                    store.clone(),
                                    block.clone(),
                                    backoff,
                                )
                            });
                            let result = futures::future::join_all(tasks).await;
                            // this block will be marked as handled if at least one handler succeeded.
                            // this because, for the failed events, we arleady tried to handle them
                            // many times (at this point), and there is no point in trying again.
                            let mark_as_handled =
                                result.iter().any(std::result::Result::is_ok);
                            // also, for all the failed event handlers, we should print what went
                            // wrong.
                            result.iter().for_each(|r| {
                                if let Err(e) = r {
                                    tracing::error!("{}", e);
                                }
                            });
                            if mark_as_handled {
                                store.set_last_block_number(
                                    chain_id, dest_block,
                                )?;
                                tracing::trace!(
                                    "event handled successfully. at #{}",
                                    dest_block
                                );
                            } else {
                                tracing::error!("Error while handling event, all handlers failed.");
                                tracing::warn!("Restarting event watcher ...");
                                // this a transient error, so we will retry again.
                                return Err(backoff::Error::transient(
                                    crate::Error::ForceRestart,
                                ));
                            }
                            // move forward.
                            store
                                .set_last_block_number(chain_id, dest_block)?;
                            tracing::trace!(
                                "Last saved block number: #{}",
                                dest_block
                            );
                        }
                        Ok(None) => {
                            tracing::error!("Block not found: {}", dest_block);
                        }
                        Err(e) => {
                            tracing::error!(
                                "Error while fetching block: {}",
                                e
                            );
                        }
                    };
                }
                tracing::trace!(
                    "Polled from #{} to #{}, should cooldown? ({})",
                    block,
                    dest_block,
                    should_cooldown
                );
                if should_cooldown {
                    let duration =
                        Duration::from_millis(listener_config.polling_interval);
                    tracing::trace!(
                        "Cooldown a bit for {}ms",
                        duration.as_millis()
                    );
                    tokio::time::sleep(duration).await;
                }
            }
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}
