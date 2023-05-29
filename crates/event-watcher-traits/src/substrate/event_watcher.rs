// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use tokio::sync::Mutex;
use webb::substrate::subxt::{config::Header, OnlineClient};
use webb_relayer_config::event_watcher::EventsWatcherConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::{metric, retry};

use super::*;

/// A type alias to extract the event handler type from the event watcher.
pub type EventHandlerFor<W, RuntimeConfig> = Box<
    dyn EventHandler<
            RuntimeConfig,
            Client = OnlineClient<RuntimeConfig>,
            Store = <W as SubstrateEventWatcher<RuntimeConfig>>::Store,
        > + Send
        + Sync,
>;

/// A trait that defines a handler for a specific set of event types.
///
/// The handlers are implemented separately from the watchers, so that we can have
/// one event watcher and many event handlers that will run in parallel.
#[async_trait::async_trait]
pub trait EventHandler<RuntimeConfig>
where
    RuntimeConfig: subxt::Config + Send + Sync + 'static,
{
    /// The Runtime Client that can be used to perform API calls.
    type Client: OnlineClientT<RuntimeConfig> + Send + Sync;
    /// The Storage backend, used by the event handler to store its state.
    type Store: HistoryStore;
    /// a method to be called with a list of events information,
    /// it is up to the handler to decide what to do with the event.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be discarded. to have a retry mechanism, use the [`EventHandlerWithRetry::handle_events_with_retry`] method
    /// which does exactly what it says.
    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (events, block_number): (subxt::events::Events<RuntimeConfig>, u64),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()>;

    /// Whether any of the events could be handled by the handler
    async fn can_handle_events(
        &self,
        events: subxt::events::Events<RuntimeConfig>,
    ) -> webb_relayer_utils::Result<bool>;
}

/// An Auxiliary trait to handle events with retry logic.
///
/// this trait is automatically implemented for all the event handlers.
#[async_trait::async_trait]
pub trait EventHandlerWithRetry<RuntimeConfig>:
    EventHandler<RuntimeConfig>
where
    RuntimeConfig: subxt::Config + Send + Sync + 'static,
{
    /// A method to be called with the list of events information,
    /// it is up to the handler to decide what to do with these events.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be retried again, depends on the retry strategy. if you do not care about the retry
    /// strategy, use the [`EventHandler::handle_events`] method instead.
    ///
    /// If this method returns Ok(true), these events will be marked as handled.
    ///
    /// **Note**: this method is automatically implemented for all the event handlers.
    async fn handle_events_with_retry(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (events, block_number): (subxt::events::Events<RuntimeConfig>, u64),
        backoff: impl backoff::backoff::Backoff + Send + Sync + 'static,
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        if !self.can_handle_events(events.clone()).await? {
            return Ok(());
        };
        let wrapped_task = || {
            self.handle_events(
                store.clone(),
                client.clone(),
                (events.clone(), block_number),
                metrics.clone(),
            )
            .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T, C> EventHandlerWithRetry<C> for T
where
    C: subxt::Config + Send + Sync + 'static,
    T: EventHandler<C> + ?Sized,
{
}

/// Represents a Substrate event watcher.
#[async_trait::async_trait]
pub trait SubstrateEventWatcher<RuntimeConfig>
where
    RuntimeConfig: subxt::Config + Send + Sync + 'static,
{
    /// A helper unique tag to help identify the event watcher in the tracing logs.
    const TAG: &'static str;

    /// The name of the pallet that this event watcher is watching.
    const PALLET_NAME: &'static str;

    /// The Storage backend, used by the event watcher to store its state.
    type Store: HistoryStore;

    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = %chain_id,
            tag = %Self::TAG
        )
    )]
    async fn run(
        &self,
        chain_id: u32,
        ctx: RelayerContext,
        store: Arc<Self::Store>,
        event_watcher_config: EventsWatcherConfig,
        handlers: Vec<EventHandlerFor<Self, RuntimeConfig>>,
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        const MAX_RETRY_COUNT: usize = 5;

        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let metrics_clone = metrics.clone();
        let task = || async {
            let maybe_client = ctx
                .substrate_provider::<RuntimeConfig, _>(chain_id)
                .await;
            let client = match maybe_client {
                Ok(client) => client,
                Err(err) => {
                    tracing::error!(
                        "Failed to connect with substrate client for chain_id: {}, retrying...!",
                        chain_id
                    );
                    return Err(backoff::Error::transient(err));
                }
            };
            let client = Arc::new(client);
            let mut instant = std::time::Instant::now();
            let step = 1u64;
            let rpc = client.rpc();
            // get pallet index
            let pallet_index = {
                let metadata = client.metadata();
                let pallet = metadata
                    .pallet(Self::PALLET_NAME)
                    .map_err(Into::into)
                    .map_err(backoff::Error::permanent)?;
                pallet.index()
            };

            // create history store key
            let src_typed_chain_id = TypedChainId::Substrate(chain_id);
            let target = SubstrateTargetSystem::builder()
                .pallet_index(pallet_index)
                .tree_id(chain_id)
                .build();
            let src_target_system = TargetSystem::Substrate(target);
            let history_store_key =
                ResourceId::new(src_target_system, src_typed_chain_id);

            loop {
                // now we start polling for new events.
                // get the current latest block number.
                let latest_head = rpc
                    .finalized_head()
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)
                    .await?;

                let maybe_latest_header = rpc
                    .header(Some(latest_head))
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)
                    .await?;
                let latest_header = if let Some(header) = maybe_latest_header {
                    header
                } else {
                    tracing::warn!("No latest header found");
                    continue;
                };
                // current finalized block number
                let current_block_number: u64 = latest_header.number().into();

                tracing::trace!(
                    "Latest block number: #{}",
                    current_block_number
                );
                let sync_blocks_from: u64 = event_watcher_config
                    .sync_blocks_from
                    .unwrap_or(current_block_number);
                // get latest saved block number
                let block = store
                    .get_last_block_number(history_store_key, sync_blocks_from)
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)?;

                let dest_block = cmp::min(block + step, current_block_number);
                // check if we are now on the latest block.
                let should_cooldown = dest_block == current_block_number;
                tracing::trace!(
                    %dest_block,
                    %current_block_number,
                    %block
                );
                tracing::trace!("Reading from #{} to #{}", block, dest_block);
                // Only handle events from found blocks if they are new
                if dest_block != block {
                    // we need to query the node for the events that happened in the
                    // range [block, dest_block].
                    // so first we get the hash of the block we want to start from.
                    let maybe_from = rpc
                        .block_hash(Some(dest_block.into()))
                        .map_err(Into::into)
                        .map_err(backoff::Error::transient)
                        .await?;

                    let from = maybe_from.unwrap_or(latest_head);
                    tracing::trace!(?from, "Querying events");
                    let events = client
                        .events()
                        .at(Some(from))
                        .map_err(Into::into)
                        .map_err(backoff::Error::transient)
                        .await?;

                    tracing::trace!("Found #{} events", events.len());
                    // wraps each handler future in a retry logic, that will retry the handler
                    // if it fails, up to `MAX_RETRY_COUNT`, after this it will ignore that event for
                    // that specific handler.
                    let tasks = handlers.iter().map(|handler| {
                        // a constant backoff with maximum retry count is used here.
                        let backoff = retry::ConstantWithMaxRetryCount::new(
                            Duration::from_millis(100),
                            MAX_RETRY_COUNT,
                        );
                        handler.handle_events_with_retry(
                            store.clone(),
                            client.clone(),
                            (events.clone(), dest_block),
                            backoff,
                            metrics_clone.clone(),
                        )
                    });
                    let result = futures::future::join_all(tasks).await;

                    // this event will be marked as handled if at least one handler succeeded.
                    // this because, for the failed events, we arleady tried to handle them
                    // many times (at this point), and there is no point in trying again.
                    let mark_as_handled = result.iter().any(Result::is_ok);
                    // also, for all the failed event handlers, we should print what went
                    // wrong.
                    result.iter().for_each(|r| {
                        if let Err(e) = r {
                            tracing::error!("{}", e);
                        }
                    });

                    if mark_as_handled {
                        store.set_last_block_number(
                            history_store_key,
                            dest_block,
                        )?;
                        tracing::trace!(
                            "event handled successfully at block #{}",
                            dest_block
                        );
                    } else {
                        tracing::error!(
                            "Error while handling event, all handlers failed."
                        );
                        tracing::warn!("Restarting event watcher ...");
                        // this a transient error, so we will retry again.
                        return Err(backoff::Error::transient(
                            webb_relayer_utils::Error::ForceRestart,
                        ));
                    }
                }
                tracing::trace!("Polled from #{} to #{}", block, dest_block);
                if should_cooldown {
                    let duration = Duration::from_secs(6);
                    tracing::trace!(
                        "Cooldown a bit for {}ms",
                        duration.as_millis()
                    );
                    tokio::time::sleep(duration).await;
                }

                let print_progress_interval = Duration::from_millis(
                    event_watcher_config.print_progress_interval,
                );

                if print_progress_interval != Duration::from_millis(0)
                    && instant.elapsed() > print_progress_interval
                {
                    // calculate sync progress.
                    let total = current_block_number as f64;
                    let current_value = dest_block as f64;
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
        // Bridge watcher backoff metric
        metrics.lock().await.bridge_watcher_back_off.inc();
        drop(metrics);
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}
