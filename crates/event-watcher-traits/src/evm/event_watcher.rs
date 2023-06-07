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
use webb::evm::ethers::prelude::TimeLag;
use webb_relayer_utils::{multi_provider::MultiProvider, retry};

use super::*;

/// Ethereum client using Ethers, that includes a retry strategy.
pub type EthersClient =
    providers::Provider<providers::RetryClient<MultiProvider<providers::Http>>>;

/// Ethereum TimeLag client using Ethers, that includes a retry strategy.
pub type EthersTimeLagClient = TimeLag<
    Arc<
        providers::Provider<
            providers::RetryClient<MultiProvider<providers::Http>>,
        >,
    >,
>;

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

/// A helper type to extract the [`EventHandler`] from the [`EventWatcher`] trait.
pub type EventHandlerFor<W> = Box<
    dyn EventHandler<
            Contract = <W as EventWatcher>::Contract,
            Events = <W as EventWatcher>::Events,
            Store = <W as EventWatcher>::Store,
        > + Send
        + Sync,
>;

/// A trait for watching events from a watchable contract.
/// EventWatcher trait exists for deployments that are smart-contract / EVM based
#[async_trait::async_trait]
pub trait EventWatcher {
    /// A Helper tag used to identify the event watcher during the logs.
    const TAG: &'static str;
    /// The contract that this event watcher is watching.
    type Contract: Deref<Target = contract::Contract<EthersTimeLagClient>>
        + WatchableContract;
    /// The Events that this event watcher is interested in.
    type Events: contract::EthLogDecode + Clone;
    /// The Storage backend that will be used to store the required state for this event watcher
    type Store: HistoryStore + EventHashStore;
    /// Returns a task that should be running in the background
    /// that will watch events
    #[tracing::instrument(
        skip_all,
        fields(
            address = %contract.address(),
            tag = %Self::TAG,
        ),
    )]
    async fn run(
        &self,
        client: Arc<EthersTimeLagClient>,
        store: Arc<Self::Store>,
        contract: Self::Contract,
        handlers: Vec<EventHandlerFor<Self>>,
        ctx: &RelayerContext,
    ) -> webb_relayer_utils::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let task = || async {
            let step = contract.max_blocks_per_step().as_u64();
            let metrics = &ctx.metrics;
            let chain_id: u32 = client
                .inner()
                .get_chainid()
                .map_err(Into::into)
                .map_err(backoff::Error::transient)
                .await?
                .as_u32();
            // now we start polling for new events.
            // create history store key
            let src_target_system = TargetSystem::new_contract_address(
                contract.address().to_fixed_bytes(),
            );
            let src_typed_chain_id = TypedChainId::Evm(chain_id);
            let history_store_key =
                ResourceId::new(src_target_system, src_typed_chain_id);

            // saves the last time we printed sync progress.
            let mut instant = std::time::Instant::now();
            // we only query this once, at the start of the events watcher.
            // then we will update it later once we fully synced.

            let mut target_block_number = client
                .get_block_number()
                .map_err(Into::into)
                .map_err(backoff::Error::transient)
                .await?
                .as_u64();

            // Save the target block number in the store
            // so other things can use it.
            store.set_target_block_number(
                history_store_key,
                target_block_number,
            )?;

            loop {
                let block = store.get_last_block_number(
                    history_store_key,
                    contract.deployed_at().as_u64(),
                )?;
                let dest_block =
                    core::cmp::min(block + step, target_block_number);

                let events_filter = contract
                    .event_with_filter::<Self::Events>(Default::default())
                    .from_block(block + 1)
                    .to_block(dest_block);
                let found_events = events_filter
                    .query_with_meta()
                    .map_err(Into::into)
                    .map_err(backoff::Error::transient)
                    .await?;

                let number_of_events = found_events.len();
                tracing::trace!("Found #{number_of_events} events");
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
                            metrics.clone(),
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
                                tracing::error!(?e, %chain_id, "Error while handling the event");
                            }
                        });
                    if mark_as_handled {
                        store.set_last_block_number(
                            history_store_key,
                            log.block_number.as_u64(),
                        )?;
                        tracing::trace!(
                            %chain_id,
                            %log.block_number,
                            "event handled successfully",
                        );
                    } else {
                        tracing::error!(%chain_id, "Error while handling event, all handlers failed.");
                        tracing::warn!(%chain_id, "Restarting event watcher ...");
                        // this a transient error, so we will retry again.
                        return Err(backoff::Error::transient(
                            webb_relayer_utils::Error::ForceRestart,
                        ));
                    }
                }

                // move the block pointer to the destination block
                store.set_last_block_number(history_store_key, dest_block)?;
                // if we fully synced, we can update the target block number
                let should_cooldown = dest_block == target_block_number;
                if should_cooldown {
                    let duration = contract.polling_interval();
                    tracing::trace!(
                        "Cooldown a bit for {}ms",
                        duration.as_millis()
                    );
                    tokio::time::sleep(duration).await;
                    // update the latest block number
                    target_block_number = client
                        .get_block_number()
                        .map_err(Into::into)
                        .map_err(backoff::Error::transient)
                        .await?
                        .as_u64();
                    store.set_target_block_number(
                        history_store_key,
                        target_block_number,
                    )?;
                }

                if contract.print_progress_interval()
                    != Duration::from_millis(0)
                    && instant.elapsed() > contract.print_progress_interval()
                {
                    let currently_at = store.get_last_block_number(
                        history_store_key,
                        contract.deployed_at().as_u64(),
                    )?;
                    let diff = currently_at.saturating_sub(block);
                    let progress = currently_at as f64
                        / target_block_number as f64
                        * 100.0;
                    let is_syncing = progress < 99.99;
                    tracing::info!(
                        target_block = target_block_number,
                        currently_at,
                        diff,
                        is_syncing,
                        progress,
                        %chain_id
                    );
                    tracing::event!(
                        target: webb_relayer_utils::probe::TARGET,
                        tracing::Level::TRACE,
                        kind = %webb_relayer_utils::probe::Kind::Sync,
                        %chain_id,
                        %block,
                        %dest_block,
                    );
                    instant = std::time::Instant::now();
                }
            }
        };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}
/// A trait that defines a handler for a specific set of event types.
///
/// The handlers are implemented separately from the watchers, so that we can have
/// one event watcher and many event handlers that will run in parallel.
#[async_trait::async_trait]
pub trait EventHandler {
    /// The Type of contract this handler is for, Must be the same as the contract type in the
    /// watcher.
    type Contract: Deref<Target = contract::Contract<EthersTimeLagClient>>
        + WatchableContract;
    /// The type of event this handler is for.
    type Events: contract::EthLogDecode + Clone;
    /// The storage backend that this handler will use.
    type Store: HistoryStore + EventHashStore;

    /// a method to be called with the event information,
    /// it is up to the handler to decide what to do with the event.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be discarded. to have a retry mechanism, use the [`EventHandlerWithRetry::handle_event_with_retry`] method
    /// which does exactly what it says.
    ///
    /// If this method returns Ok(true), the event will be marked as handled.
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, contract::LogMeta),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()>;

    /// Whether any of the events could be handled by the handler
    async fn can_handle_events(
        &self,
        (event, log): (Self::Events, contract::LogMeta),
        wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool>;
}

/// An Auxiliary trait to handle events with retry logic.
///
/// this trait is automatically implemented for all the event handlers.
#[async_trait::async_trait]
pub trait EventHandlerWithRetry: EventHandler {
    /// A method to be called with the event information,
    /// it is up to the handler to decide what to do with the event.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be retried again, depends on the retry strategy. if you do not care about the retry
    /// strategy, use the [`EventHandler::handle_event`] method instead.
    ///
    /// If this method returns Ok(true), the event will be marked as handled.
    ///
    /// **Note**: this method is automatically implemented for all the event handlers.
    async fn handle_event_with_retry(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, contract::LogMeta),
        backoff: impl backoff::backoff::Backoff + Send + Sync + 'static,
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        if !self
            .can_handle_events((event.clone(), log.clone()), contract)
            .await?
        {
            return Ok(());
        };

        let wrapped_task = || {
            self.handle_event(
                store.clone(),
                contract,
                (event.clone(), log.clone()),
                metrics.clone(),
            )
            .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T> EventHandlerWithRetry for T where T: EventHandler + ?Sized {}
