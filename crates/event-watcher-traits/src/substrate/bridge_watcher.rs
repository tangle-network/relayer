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

use super::{event_watcher::SubstrateEventWatcher, *};

// A Substrate Bridge Watcher is a trait for Signature Bridge Pallet that is not specific for watching events from that pallet,
/// instead it watches for commands sent from other event watchers or services, it helps decouple the event watchers
/// from the actual action that should be taken depending on the event.
#[async_trait::async_trait]
pub trait SubstrateBridgeWatcher<RuntimeConfig>:
    SubstrateEventWatcher<RuntimeConfig>
where
    RuntimeConfig: subxt::Config + Send + Sync + 'static,
    Self::Store: ProposalStore<Proposal = ()>
        + QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    /// A method that is called when a command is received that needs to be
    /// handled and executed.
    async fn handle_cmd(
        &self,
        chain_id: u32,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        cmd: BridgeCommand,
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
        chain_id: u32,
        client: Arc<Self::Client>,
        store: Arc<Self::Store>,
    ) -> webb_relayer_utils::Result<()> {
        let backoff = backoff::backoff::Constant::new(Duration::from_secs(1));

        let task = || async {
            let my_chain_id = webb_proposals::TypedChainId::Substrate(chain_id);
            let bridge_key = BridgeKey::new(my_chain_id);
            let key = SledQueueKey::from_bridge_key(bridge_key);
            loop {
                let result = match store.dequeue_item(key)? {
                    Some(cmd) => {
                        self.handle_cmd(
                            chain_id,
                            store.clone(),
                            client.clone(),
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
