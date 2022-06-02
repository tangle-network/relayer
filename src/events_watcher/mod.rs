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
//
#![warn(missing_docs)]
//! # Relayer Events Watcher Module 🕸️
//!
//! A module that listens for events on a given chain.
//!
//! ## Overview
//!
//! Event watcher traits handle the syncing and listening of events for a given network.
//! The event watcher calls into a storage for handling of important state. The run implementation
//! of an event watcher polls for blocks. Implementations of the event watcher trait define an
//! action to take when the specified event is found in a block at the `handle_event` api.
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
        types::transaction,
    },
    substrate::{
        scale,
        subxt::{self, sp_runtime::traits::Header},
    },
};

use crate::store::sled::SledQueueKey;
use crate::store::{
    BridgeCommand, BridgeKey, EventHashStore, HistoryStore, ProposalStore,
    QueueStore,
};

/// A module for listening on proposal events.
mod proposal_handler_watcher;
#[doc(hidden)]
pub use proposal_handler_watcher::*;

/// A module for listening on Signature Bridge commands and events.
mod signature_bridge_watcher;
#[doc(hidden)]
pub use signature_bridge_watcher::*;

/// A module for listening on DKG Governor Changes event.
mod governor_watcher;
#[doc(hidden)]
pub use governor_watcher::*;

#[doc(hidden)]
pub mod proposal_signing_backend;

/// A module for listening on substrate events.
#[doc(hidden)]
pub mod substrate;

/// A module for listening on evm events.
#[doc(hidden)]
pub mod evm;

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

/// A trait for watching events from a watchable contract.
/// EventWatcher trait exists for deployments that are smart-contract / EVM based
#[async_trait::async_trait]
pub trait EventWatcher {
    const TAG: &'static str;
    type Middleware: providers::Middleware + 'static;
    type Contract: Deref<Target = contract::Contract<Self::Middleware>>
        + WatchableContract;
    type Events: contract::EthLogDecode;
    type Store: HistoryStore + EventHashStore;

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
                        tracing::Level::TRACE,
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

/// A Bridge Watcher is a trait for Bridge contracts that not specific for watching events from that contract,
/// instead it watches for commands sent from other event watchers or services, it helps decouple the event watchers
/// from the actual action that should be taken depending on the event.
#[async_trait::async_trait]
pub trait BridgeWatcher: EventWatcher
where
    Self::Store: ProposalStore<Proposal = ()>
        + QueueStore<transaction::eip2718::TypedTransaction, Key = SledQueueKey>
        + QueueStore<BridgeCommand, Key = SledQueueKey>,
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
            let bridge_key = BridgeKey::new(my_address, my_chain_id);
            let key = SledQueueKey::from_bridge_key(bridge_key);
            while let Some(command) = store.dequeue_item(key)? {
                let result =
                    self.handle_cmd(store.clone(), &contract, command).await;
                match result {
                    Ok(_) => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Error while handle_cmd {}", e);
                        // this a transient error, so we will retry again.
                        tracing::warn!("Restarting bridge event watcher ...");
                        return Err(backoff::Error::transient(e));
                    }
                }
                // sleep for a bit to avoid overloading the db.
            }
            // whenever this loop stops, we will restart the whole task again.
            // that way we never have to worry about closed channels.
            Err(backoff::Error::transient(anyhow::anyhow!("Restarting")))
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}

/// Type alias for Substrate block number.
pub type BlockNumberOf<T> =
    <<T as SubstrateEventWatcher>::RuntimeConfig as subxt::Config>::BlockNumber;

/// Represents a Substrate event watcher.
#[async_trait::async_trait]
pub trait SubstrateEventWatcher {
    const TAG: &'static str;
    /// The Config of this Runtime, mostly it will be [`subxt::DefaultConfig`]
    type RuntimeConfig: subxt::Config + Send + Sync + 'static;
    /// The Runtime API.
    type Api: From<subxt::Client<Self::RuntimeConfig>> + Send + Sync;
    /// All types of events that are supported by this Runtime.
    /// Usually it will be [`my_runtime::api::Event`] which is an enum of all events.
    type Event: scale::Decode + Send + Sync + 'static;
    /// The kind of event that this watcher is watching.
    type FilteredEvent: subxt::Event + Send + Sync + 'static;
    type Store: HistoryStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
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

        let task = || async {
            let mut instant = std::time::Instant::now();
            let step = U64::from(1u64);
            let client_api = client.clone();
            let api: Arc<Self::Api> = Arc::new(client_api.to_runtime_api());
            let rpc = client.rpc();
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
                    tracing::trace!(?from, "Querying events");
                    let events =
                        subxt::events::at::<_, Self::Event>(&client, from)
                            .map_err(anyhow::Error::from)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use webb::substrate::dkg_runtime;
    use webb::substrate::dkg_runtime::api::system;

    use crate::store::sled::SledStore;

    use super::*;

    #[derive(Debug, Clone, Default)]
    struct RemarkedEventWatcher;

    #[async_trait::async_trait]
    impl SubstrateEventWatcher for RemarkedEventWatcher {
        const TAG: &'static str = "Remarked Event Watcher";

        type RuntimeConfig = subxt::DefaultConfig;

        type Api = dkg_runtime::api::RuntimeApi<
            Self::RuntimeConfig,
            subxt::PolkadotExtrinsicParams<Self::RuntimeConfig>,
        >;

        type Event = dkg_runtime::api::Event;
        type FilteredEvent = system::events::Remarked;

        type Store = SledStore;

        async fn handle_event(
            &self,
            _store: Arc<Self::Store>,
            _api: Arc<Self::Api>,
            (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        ) -> anyhow::Result<()> {
            tracing::debug!(
                "Received `Remarked` Event: {:?} at block number: #{}",
                event,
                block_number
            );
            Ok(())
        }
    }

    fn setup_logger() -> anyhow::Result<()> {
        let log_level = tracing::Level::TRACE;
        let env_filter = tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(format!("webb_relayer={}", log_level).parse()?);
        tracing_subscriber::fmt()
            .with_target(true)
            .without_time()
            .with_max_level(log_level)
            .with_env_filter(env_filter)
            .with_test_writer()
            .compact()
            .init();
        Ok(())
    }

    #[tokio::test]
    #[ignore = "need to be run manually"]
    async fn substrate_event_watcher_should_work() -> anyhow::Result<()> {
        setup_logger()?;
        let node_name = String::from("test-node");
        let chain_id = U256::from(5u32);
        let store = Arc::new(SledStore::temporary()?);
        let client = subxt::ClientBuilder::new().build().await?;
        let watcher = RemarkedEventWatcher::default();
        watcher.run(node_name, chain_id, client, store).await?;
        Ok(())
    }
}
