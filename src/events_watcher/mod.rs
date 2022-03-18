use std::cmp;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use ethereum_types::{U256, U64};
use futures::prelude::*;
use webb::{
    evm::ethers::{
        contract,
        providers::{self, Middleware},
        types,
    },
    substrate::{
        scale,
        subxt::{
            self,
            sp_core::{storage::StorageKey, twox_128},
            sp_runtime::traits::Header,
        },
    },
};

use crate::store::HistoryStore;
use crate::utils;

mod tornado_leaves_watcher;
pub use tornado_leaves_watcher::*;

mod anchor_watcher_over_dkg;
pub use anchor_watcher_over_dkg::*;

mod proposal_handler_watcher;
pub use proposal_handler_watcher::*;

/// A watchable contract is a contract used in the [EventWatcher]
pub trait WatchableContract: Send + Sync {
    /// The block number where this contract is deployed.
    fn deployed_at(&self) -> types::U64;

    /// How often this contract should be polled for events.
    fn polling_interval(&self) -> Duration;

    /// How many events to fetch at one request.
    fn max_events_per_step(&self) -> types::U64;

    /// The frequency of printing the sync progress.
    fn print_progress_interval(&self) -> Duration;
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
            let step = contract.max_events_per_step();
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
                        .from_block(block + 1)
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
                    tracing::trace!("Last saved block number: #{}", dest_block);
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

                // only print the progress if 7 seconds (by default) is passed.
                if contract.print_progress_interval()
                    != Duration::from_millis(0)
                    && instant.elapsed() > contract.print_progress_interval()
                {
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
                    tracing::event!(
                        target: crate::probe::TARGET,
                        tracing::Level::DEBUG,
                        kind = %crate::probe::Kind::Sync,
                        %block,
                        %dest_block,
                        %sync_progress,
                    );
                    instant = std::time::Instant::now();
                }
            }
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}

pub type BlockNumberOf<T> =
    <<T as SubstrateEventWatcher>::RuntimeConfig as subxt::Config>::BlockNumber;

#[async_trait::async_trait]
pub trait SubstrateEventWatcher {
    const TAG: &'static str;
    type RuntimeConfig: subxt::Config + Send + Sync + 'static;
    type Api: From<subxt::Client<Self::RuntimeConfig>> + Send + Sync;
    type Event: subxt::Event + Send + Sync;
    type Store: HistoryStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()>;

    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            node = %node_name,
            chain_id = %chain_id,
            tag = %Self::TAG
        )
    )]
    async fn run(
        &self,
        node_name: String,
        chain_id: U256,
        client: subxt::Client<Self::RuntimeConfig>,
        store: Arc<Self::Store>,
    ) -> anyhow::Result<()> {
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        // The storage Key, where all events are stored.
        struct SystemEvents(StorageKey);

        impl Default for SystemEvents {
            fn default() -> Self {
                let mut storage_key = twox_128(b"System").to_vec();
                storage_key.extend(twox_128(b"Events").to_vec());
                Self(StorageKey(storage_key))
            }
        }

        impl From<SystemEvents> for StorageKey {
            fn from(key: SystemEvents) -> Self {
                key.0
            }
        }

        let task = || async {
            let mut instant = std::time::Instant::now();
            let step = U64::from(50u64);
            let client_api = client.clone();
            let api: Arc<Self::Api> = Arc::new(client_api.to_runtime_api());
            let rpc = client.rpc();
            let decoder = client.events_decoder();
            let keys = vec![StorageKey::from(SystemEvents::default())];
            loop {
                // now we start polling for new events.
                // get the latest seen block number.
                let block = store.get_last_block_number(
                    (node_name.clone(), chain_id),
                    1u64.into(),
                )?;
                let latest_head =
                    rpc.finalized_head().map_err(anyhow::Error::from).await?;
                let maybe_latest_header = rpc
                    .header(Some(latest_head))
                    .map_err(anyhow::Error::from)
                    .await?;
                let latest_header = if let Some(header) = maybe_latest_header {
                    header
                } else {
                    tracing::warn!("No latest header found");
                    continue;
                };
                let current_block_number_bytes =
                    scale::Encode::encode(&latest_header.number());
                let current_block_number: u32 =
                    scale::Decode::decode(&mut &current_block_number_bytes[..])
                        .map_err(anyhow::Error::from)?;
                let current_block_number = U64::from(current_block_number);
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
                        .block_hash(Some(block.as_u32().into()))
                        .map_err(anyhow::Error::from)
                        .await?;
                    let from = maybe_from.unwrap_or(latest_head);
                    let to = rpc
                        .block_hash(Some(dest_block.as_u32().into()))
                        .map_err(anyhow::Error::from)
                        .await?;
                    // then we query the storage set of the system events.
                    let change_sets = rpc
                        .query_storage(keys.clone(), from, to)
                        .map_err(anyhow::Error::from)
                        .await?;
                    // now we go through the changeset, and for every change we extract the events.
                    let found_events = change_sets
                        .into_iter()
                        .flat_map(|c| utils::change_set_to_events(c, decoder))
                        .collect::<Vec<_>>();
                    tracing::trace!("Found #{} events", found_events.len());

                    for (block_hash, event) in found_events {
                        let maybe_header = rpc
                            .header(Some(block_hash))
                            .map_err(anyhow::Error::from)
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
                                api.clone(),
                                (event, block_number),
                            )
                            .await;
                        match result {
                            Ok(_) => {
                                let current_block_number_bytes =
                                    scale::Encode::encode(&block_number);
                                let current_block_number: u32 =
                                    scale::Decode::decode(
                                        &mut &current_block_number_bytes[..],
                                    )
                                    .map_err(anyhow::Error::from)?;
                                let current_block_number =
                                    U64::from(current_block_number);
                                store.set_last_block_number(
                                    (node_name.clone(), chain_id),
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
                    store.set_last_block_number(
                        (node_name.clone(), chain_id),
                        dest_block,
                    )?;
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
