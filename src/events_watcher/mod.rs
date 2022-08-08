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
use ethereum_types::{U256, U64};
use futures::prelude::*;
use std::cmp;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

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

/// A module for listening on dkg events.
#[doc(hidden)]
pub mod dkg;

/// A module for listening on substrate events.
#[doc(hidden)]
pub mod substrate;

/// A module for listening on evm events.
#[doc(hidden)]
pub mod evm;

/// A module that contains helpers for retry logic.
#[doc(hidden)]
mod retry;

/// A watchable contract is a contract used in the [EventWatcher]
pub trait WatchableContract: Send + Sync {
    /// The block number where this contract is deployed.
    fn deployed_at(&self) -> types::U64;

    /// How often this contract should be polled for events.
    fn polling_interval(&self) -> Duration;

    /// How many events to fetch at one request.
    fn max_blocks_per_step(&self) -> types::U64;

    /// The frequency of printing the sync progress.
    fn print_progress_interval(&self) -> Duration;
}

pub type EventHandlerFor<W> = Box<
    dyn EventHandler<
            Middleware = <W as EventWatcher>::Middleware,
            Contract = <W as EventWatcher>::Contract,
            Events = <W as EventWatcher>::Events,
            Store = <W as EventWatcher>::Store,
        > + Send
        + Sync,
>;

/// A trait for watching events from a watchable contract.
/// EventWatcher trait exists for deployments that are smart-contract / EVM based
#[async_trait::async_trait]
pub trait EventWatcher
where
    <Self::Middleware as providers::Middleware>::Error: Into<crate::Error>,
{
    const TAG: &'static str;
    type Middleware: providers::Middleware + 'static;
    type Contract: Deref<Target = contract::Contract<Self::Middleware>>
        + WatchableContract;
    type Events: contract::EthLogDecode + Clone;
    type Store: HistoryStore + EventHashStore;
    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = ?client.get_chainid().await,
            address = %contract.address(),
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<Self::Middleware>,
        store: Arc<Self::Store>,
        contract: Self::Contract,
        handlers: Vec<EventHandlerFor<Self>>,
    ) -> crate::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let task = || async {
            let step = contract.max_blocks_per_step();
            // saves the last time we printed sync progress.
            let mut instant = std::time::Instant::now();
            let chain_id = client.get_chainid().map_err(Into::into).await?;
            // now we start polling for new events.
            loop {
                let block = store.get_last_block_number(
                    (chain_id, contract.address()),
                    contract.deployed_at(),
                )?;
                let current_block_number =
                    client.get_block_number().map_err(Into::into).await?;
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
                        .map_err(Into::into)
                        .await?;

                    tracing::trace!("Found #{} events", found_events.len());

                    for (event, log) in found_events {
                        // wraps each handler future in a retry logic, that will retry the handler
                        // if it fails, up to `MAX_RETRY_COUNT`, after this it will ignore that event for
                        // that specific handler.
                        const MAX_RETRY_COUNT: usize = 5;
                        let tasks = handlers.iter().map(|handler| {
                            // a constant backoff with maximum retry count is used here.
                            let backoff = retry::ConstantWithMaxRetryCount::new(
                                Duration::from_millis(100),
                                MAX_RETRY_COUNT,
                            );
                            handler.handle_event_with_retry(
                                store.clone(),
                                &contract,
                                (event.clone(), log.clone()),
                                backoff,
                            )
                        });
                        let result = futures::future::join_all(tasks).await;
                        // this event will be marked as handled if at least one handler succeeded.
                        // this because, for the failed events, we arleady tried to handle them
                        // many times (at this point), and there is no point in trying again.
                        let mark_as_handled = result.iter().any(|r| r.is_ok());
                        // also, for all the failed event handlers, we should print what went
                        // wrong.
                        result.iter().for_each(|r| {
                            if let Err(e) = r {
                                tracing::error!("{}", e);
                            }
                        });
                        if mark_as_handled {
                            store.set_last_block_number(
                                (chain_id, contract.address()),
                                log.block_number,
                            )?;
                            tracing::trace!(
                                "event handled successfully. at #{}",
                                log.block_number
                            );
                        } else {
                            tracing::error!("Error while handling event, all handlers failed.");
                            tracing::warn!("Restarting event watcher ...");
                            // this a transient error, so we will retry again.
                            return Err(backoff::Error::transient(
                                crate::Error::ForceRestart,
                            ));
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
        if let Err(e) = backoff::future::retry(backoff, task).await {
            tracing::error!("{}", e);
            return Err(crate::Error::TaskStoppedUpnormally);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait EventHandler {
    type Middleware: providers::Middleware + 'static;
    type Contract: Deref<Target = contract::Contract<Self::Middleware>>
        + WatchableContract;
    type Events: contract::EthLogDecode + Clone;
    type Store: HistoryStore + EventHashStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, contract::LogMeta),
    ) -> crate::Result<()>;
}

/// An Auxiliary trait to handle events with retry logic.
///
/// this trait is automatically implemented for all the event handlers.
#[async_trait::async_trait]
trait EventHandlerWithRetry: EventHandler {
    async fn handle_event_with_retry(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, contract::LogMeta),
        backoff: impl backoff::backoff::Backoff + Send + Sync + 'static,
    ) -> crate::Result<()> {
        let wrapped_task = || {
            self.handle_event(
                store.clone(),
                contract,
                (event.clone(), log.clone()),
            )
            .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T> EventHandlerWithRetry for T where T: EventHandler + ?Sized {}

/// A Bridge Watcher is a trait for Bridge contracts that not specific for watching events from that contract,
/// instead it watches for commands sent from other event watchers or services, it helps decouple the event watchers
/// from the actual action that should be taken depending on the event.
#[async_trait::async_trait]
pub trait BridgeWatcher: EventWatcher
where
    Self::Store: ProposalStore<Proposal = ()>
        + QueueStore<transaction::eip2718::TypedTransaction, Key = SledQueueKey>
        + QueueStore<BridgeCommand, Key = SledQueueKey>,
    <Self::Middleware as providers::Middleware>::Error: Into<crate::Error>,
{
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        cmd: BridgeCommand,
    ) -> crate::Result<()>;

    /// Returns a task that should be running in the background
    /// that will watch for all commands
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = ?client.get_chainid().await,
            address = %contract.address(),
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<Self::Middleware>,
        store: Arc<Self::Store>,
        contract: Self::Contract,
    ) -> crate::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let task = || async {
            let my_address = contract.address();
            let my_chain_id = client.get_chainid().map_err(Into::into).await?;
            let bridge_key = BridgeKey::new(my_address, my_chain_id);
            let key = SledQueueKey::from_bridge_key(bridge_key);
            loop {
                let result = match store.dequeue_item(key)? {
                    Some(cmd) => {
                        self.handle_cmd(store.clone(), &contract, cmd).await
                    }
                    None => {
                        // yeild back to the runtime, to allow for other tasks
                        // to make progress, instead of this busy loop.
                        tokio::task::yield_now().await;
                        // the small sleep here just in case the runtime decides to
                        // run this task again immediately.
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                };
                match result {
                    Ok(_) => {
                        tracing::debug!(?key, "Handled command successfully");
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Error while handle_cmd {}", e);
                        // this a transient error, so we will retry again.
                        tracing::warn!("Restarting bridge event watcher ...");
                        return Err(backoff::Error::transient(e));
                    }
                }
            }
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
    ) -> crate::Result<()>;

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
    ) -> crate::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));

        let task = || async {
            let mut instant = std::time::Instant::now();
            let step = U64::from(1u64);
            let client_api = client.clone();
            let api: Arc<Self::Api> = Arc::new(client_api.to_runtime_api());
            let rpc = client.rpc();
            loop {
                // now we start polling for new events.
                // get the latest seen block number.
                let block = store
                    .get_last_block_number(
                        (node_name.clone(), chain_id),
                        1u64.into(),
                    )
                    .map_err(Into::into)?;
                let latest_head =
                    rpc.finalized_head().map_err(Into::into).await?;
                let maybe_latest_header =
                    rpc.header(Some(latest_head)).map_err(Into::into).await?;
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
                        .map_err(Into::into)?;
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
                        .map_err(Into::into)
                        .await?;
                    let from = maybe_from.unwrap_or(latest_head);
                    tracing::trace!(?from, "Querying events");
                    let events =
                        subxt::events::at::<_, Self::Event>(&client, from)
                            .map_err(Into::into)
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
                                    .map_err(Into::into)?;
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

// A Substrate Bridge Watcher is a trait for Signature Bridge Pallet that is not specific for watching events from that pallet,
/// instead it watches for commands sent from other event watchers or services, it helps decouple the event watchers
/// from the actual action that should be taken depending on the event.
#[async_trait::async_trait]
pub trait SubstrateBridgeWatcher: SubstrateEventWatcher
where
    Self::Store: ProposalStore<Proposal = ()>
        + QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    async fn handle_cmd(
        &self,
        chain_id: U256,
        store: Arc<Self::Store>,
        client: Arc<Self::Api>,
        cmd: BridgeCommand,
    ) -> crate::Result<()>;

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
        chain_id: U256,
        client: subxt::Client<Self::RuntimeConfig>,
        store: Arc<Self::Store>,
    ) -> crate::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));

        let task = || async {
            let client_api = client.clone();
            let api: Arc<Self::Api> = Arc::new(client_api.to_runtime_api());
            // chain_id is used as tree_id, to ensure that we have one signature bridge
            let target_system =
                webb_proposals::TargetSystem::new_tree_id(chain_id.as_u32());
            let my_chain_id =
                webb_proposals::TypedChainId::Substrate(chain_id.as_u32());
            let bridge_key = BridgeKey::new(target_system, my_chain_id);
            let key = SledQueueKey::from_bridge_key(bridge_key);
            loop {
                let result = match store.dequeue_item(key)? {
                    Some(cmd) => {
                        self.handle_cmd(
                            chain_id,
                            store.clone(),
                            api.clone(),
                            cmd,
                        )
                        .await
                    }
                    None => {
                        // yeild back to the runtime, to allow for other tasks
                        // to make progress, instead of this busy loop.
                        tokio::task::yield_now().await;
                        // the small sleep here just in case the runtime decides to
                        // run this task again immediately.
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                };

                match result {
                    Ok(_) => {
                        tracing::debug!(?key, "Handled command successfully");
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Error while handle_cmd {}", e);
                        // this a transient error, so we will retry again.
                        tracing::warn!("Restarting bridge event watcher ...");
                        return Err(backoff::Error::transient(e));
                    }
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
        ) -> crate::Result<()> {
            tracing::debug!(
                "Received `Remarked` Event: {:?} at block number: #{}",
                event,
                block_number
            );
            Ok(())
        }
    }

    fn setup_logger() -> crate::Result<()> {
        let log_level = tracing::Level::TRACE;
        let env_filter = tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(
                format!("webb_relayer={}", log_level)
                    .parse()
                    .expect("Valid filter"),
            );
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
    async fn substrate_event_watcher_should_work() -> crate::Result<()> {
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
