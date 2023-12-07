use futures::prelude::*;
use std::sync::Arc;

use webb_relayer_store::HistoryStore;

use webb_eth2_pallet_init::init_pallet::{get_typed_chain_id, init_pallet};
use webb_eth2_pallet_init::substrate_pallet_client::{setup_api, EthClientPallet};
use eth2_to_substrate_relay::config::Config;
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
            self.handle_interval(chain_id, store.clone())
                .map_err(backoff::Error::transient)
        };
        backoff::future::retry(backoff, wrapped_task).await?;
        Ok(())
    }
}

impl<T> LightClientHandlerWithRetry for T where T: LightClientHandler + ?Sized {}
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
            tag = %Self::TAG,
        ),
    )]
    async fn run(&self, config: Config) -> crate::Result<()> {
        let api = setup_api().await.map_err(std_err)?;
        if config.path_to_signer_secret_key == "NaN" {
            return Err(webb_relayer_utils::Error::Generic(
                "Secret key path must be set",
            ));
        }

        // read the path
        let path_to_suri = &config.path_to_signer_secret_key;
        let suri = std::fs::read_to_string(path_to_suri)?;
        let suri = suri.trim();

        let typed_chain_id = get_typed_chain_id(&config.clone().into());
        let mut eth_client_contract =
            EthClientPallet::new_with_suri_key(api, suri, typed_chain_id)
                .map_err(|err| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("{err:?}"),
                    )
                })?;

        let is_initialized = eth_client_contract
            .is_initialized(typed_chain_id)
            .await
            .expect("Error checking initialization");

        // Step 1: init pallet if not initialized
        if !is_initialized {
            init_pallet(&config.clone().into(), &mut eth_client_contract)
                .await
                .expect("Error on contract initialization");
            // We removed the sleep (30s) from the end of the init_pallet function. In its place,
            // we sleep here for 1/10th the time (for now)
            tracing::info!("Init pallet success (waiting 3s...)");
            tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
        } else {
            tracing::info!("Pallet already initialized...");
        }

        // Step 2: init relay
        let submit_only_finalized_blocks = true;
        let enable_binsearch = true;
        let mut relay = eth2_to_substrate_relay::eth2substrate_relay::Eth2SubstrateRelay::init(&config, Box::new(eth_client_contract), enable_binsearch, submit_only_finalized_blocks).await;

        tracing::info!("Init relay success");
        // Step 3: run relay
        relay.run(None).await;

        tracing::warn!("Finished running the relayer ...");
        Ok(())
    }
}

fn std_err<T: std::fmt::Debug>(err: T) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, format!("{err:?}"))
}
