// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use super::{event_watcher::EventWatcher, *};
use tokio::sync::Mutex;
use webb_relayer_types::EthersTimeLagClient;

/// A Bridge Watcher is a trait for Bridge contracts that not specific for watching events from that contract,
/// instead it watches for commands sent from other event watchers or services, it helps decouple the event watchers
/// from the actual action that should be taken depending on the event.
#[async_trait::async_trait]
pub trait BridgeWatcher: EventWatcher
where
    Self::Store: QueueStore<transaction::eip2718::TypedTransaction, Key = SledQueueKey>
        + QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    /// A method to be called with the [`BridgeCommand`] information to
    /// be executed by the Bridge command handler.
    ///
    /// If this method returned an error, the handler will be considered as failed and will
    /// be retry again, depends on the retry strategy.
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        cmd: BridgeCommand,
    ) -> webb_relayer_utils::Result<()>;

    /// Returns a task that should be running in the background
    /// that will watch for all commands
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
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));
        let task = || async {
            let chain_id = client
                .get_chainid()
                .map_err(Into::into)
                .map_err(backoff::Error::transient)
                .await?;
            let typed_chain_id =
                webb_proposals::TypedChainId::Evm(chain_id.as_u32());
            let bridge_key = BridgeKey::new(typed_chain_id);
            let key = SledQueueKey::from_bridge_key(bridge_key);
            loop {
                let result = match store.dequeue_item(key)? {
                    Some(item) => {
                        self.handle_cmd(store.clone(), &contract, item.inner())
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
                        tracing::debug!(?key, %chain_id, "Handled command successfully");
                        continue;
                    }
                    Err(e) => {
                        tracing::error!(%chain_id, "Error while handle_cmd {}", e);
                        // this a transient error, so we will retry again.
                        tracing::warn!(%chain_id, "Restarting bridge event watcher ...");
                        // metric for when the bridge watcher enters back off
                        let metrics = metrics.lock().await;
                        metrics.bridge_watcher_back_off.inc();
                        drop(metrics);
                        return Err(backoff::Error::transient(e));
                    }
                }
            }
        };
        // Bridge watcher backoff metric
        let metrics = metrics.lock().await;
        metrics.bridge_watcher_back_off.inc();
        drop(metrics);

        backoff::future::retry(backoff, task).await?;
        Ok(())
    }
}
