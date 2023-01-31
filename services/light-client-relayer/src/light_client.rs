use futures::prelude::*;
use std::sync::Arc;
use webb_relayer_config::block_poller::BlockPollerConfig;

use webb::{
    evm::ethers::{
        providers::{self, Middleware},
    },
};

use webb_relayer_store::HistoryStore;

/// A trait that defines a handler for a specific set of event types.
///
/// The handlers are implemented separately from the watchers, so that we can have
/// one event watcher and many event handlers that will run in parallel.
#[async_trait::async_trait]
pub trait LightClientHandler {
    /// The storage backend that this handler will use.
    type Store: HistoryStore;
    /// A method to be called to execute arbitrary code on each interval.
    /// The interval is defined by executor of this method.
    async fn handle_interval(
        &self,
        chain_id: u32,
        store: Arc<Self::Store>,
    ) -> crate::Result<()>;
}

/// An Auxiliary trait to handle events with retry logic.
///
/// this trait is automatically implemented for all the event handlers.
#[async_trait::async_trait]
pub trait LightClientHandlerWithRetry: LightClientHandler {
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
    async fn handle_interval_with_retry(
        &self,
        chain_id: u32,
        store: Arc<Self::Store>,
        backoff: impl backoff::backoff::Backoff + Send + Sync + 'static,
    ) -> crate::Result<()> {
        let wrapped_task = || {
            self.handle_interval(chain_id.clone(), store.clone())
                .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T> LightClientHandlerWithRetry for T where T: LightClientHandler + ?Sized {}

pub type LightClientHandlerFor<W> = Box<
    dyn LightClientHandler<Store = <W as LightClientPoller>::Store>
        + Send
        + Sync,
>;

/// A trait for watching block headers using a provider.
/// LightClientPoller trait exists for EVM based
#[async_trait::async_trait]
pub trait LightClientPoller {
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
        client: Arc<crate::light_client::providers::Provider<providers::Http>>,
        store: Arc<Self::Store>,
        listener_config: BlockPollerConfig,
    ) -> crate::Result<()> {
         let eth2SubstrateRelayer = Eth2SubstrateRelay::new(client.clone(), store.clone());
         eth2SubstrateRelayer.run().await?;
        Ok(())
    }
}
