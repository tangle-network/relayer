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
use webb_relayer_utils::metric;

use super::*;

/// Represents a Substrate event watcher.
#[async_trait::async_trait]
pub trait SubstrateEventWatcher {
    /// A helper unique tag to help identify the event watcher in the tracing logs.
    const TAG: &'static str;
    /// The Config of this Runtime [`subxt::PolkadotConfig`, `subxt::SubstrateConfig`]
    type RuntimeConfig: subxt::Config + Send + Sync + 'static;
    /// The Runtime Client that can be used to perform API calls.
    type Client: OnlineClientT<Self::RuntimeConfig> + Send + Sync;
    /// All types of events that are supported by this Runtime.
    /// Usually it will be [`my_runtime::api::Event`] which is an enum of all events.
    type Event: scale::Decode + Send + Sync + 'static;
    /// The kind of event that this watcher is watching.
    type FilteredEvent: subxt::events::StaticEvent + Send + Sync + 'static;
    /// The Storage backend, used by the event watcher to store its state.
    type Store: HistoryStore;

    /// A method to be called with the event information,
    /// it is up to the handler to decide what to do with the event.
    /// If this method returned an error, the handler will be considered as failed and will
    /// be retried again, depends on the retry strategy.
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()>;

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
        _node_name: String,
        chain_id: u32,
        client: Arc<Self::Client>,
        store: Arc<Self::Store>,
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let metrics_clone = metrics.clone();
        let task = || async {
            let mut instant = std::time::Instant::now();
            let step = 1u64;
            let rpc = client.rpc();
            loop {
                // now we start polling for new events.
                // get the latest seen block number.

                // create history store key
                let src_typed_chain_id = TypedChainId::Substrate(chain_id);
                let target = SubstrateTargetSystem::builder()
                    .pallet_index(chain_id as u8)
                    .tree_id(chain_id)
                    .build();
                let src_target_system = TargetSystem::Substrate(target);
                let history_store_key =
                    ResourceId::new(src_target_system, src_typed_chain_id);

                let block = store
                    .get_last_block_number(history_store_key, 1u64)
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)?;
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
                let current_block_number: u64 =
                    (*latest_header.number()).into();

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
                    // we need to query the node for the events that happened in the
                    // range [block, dest_block].
                    // so first we get the hash of the block we want to start from.
                    let maybe_from = rpc
                        .block_hash(Some(block.into()))
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

                    let found_events = events
                        .find::<Self::FilteredEvent>()
                        .flatten()
                        .map(|e| (from, e))
                        .collect::<Vec<_>>();
                    tracing::trace!("Found #{} events", found_events.len());

                    for (block_hash, event) in found_events {
                        let maybe_header = rpc
                            .header(Some(block_hash))
                            .map_err(Into::into)
                            .map_err(backoff::Error::transient)
                            .await?;
                        let header = if let Some(header) = maybe_header {
                            header
                        } else {
                            tracing::warn!(
                                "No header found for block #{:?}",
                                block_hash
                            );
                            continue;
                        };
                        let block_number = *header.number();
                        let result = self
                            .handle_event(
                                store.clone(),
                                client.clone(),
                                (event, block_number),
                                metrics_clone.clone(),
                            )
                            .await;
                        match result {
                            Ok(_) => {
                                let current_block_number: u64 =
                                    block_number.into();

                                store.set_last_block_number(
                                    history_store_key,
                                    current_block_number,
                                )?;
                                tracing::trace!(
                                    "event handled successfully. at #{}",
                                    current_block_number
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
                    store
                        .set_last_block_number(history_store_key, dest_block)?;
                    tracing::trace!("Last saved block number: #{}", dest_block);
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
                // only print the progress if 7 seconds (by default) is passed.
                if instant.elapsed() > Duration::from_secs(7) {
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
