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

use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::subxt::{self, Config, OnlineClient};
use webb::substrate::tangle_runtime::api::system;
use webb_relayer_config::event_watcher::EventsWatcherConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_store::sled::SledStore;
use webb_relayer_utils::{metric, TangleRuntimeConfig};

use crate::substrate::EventHandler;
use crate::SubstrateEventWatcher;

#[derive(Debug, Clone, Default)]
struct TestEventsWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<TangleRuntimeConfig> for TestEventsWatcher {
    const TAG: &'static str = "Test Event Watcher";

    const PALLET_NAME: &'static str = "System";

    type Store = SledStore;
}

#[derive(Debug, Clone, Default)]
struct RemarkedEventHandler;

#[async_trait::async_trait]
impl<TangleRuntimeConfig: Sync + Send + Config>
    EventHandler<TangleRuntimeConfig> for RemarkedEventHandler
{
    type Client = OnlineClient<TangleRuntimeConfig>;
    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<TangleRuntimeConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.has::<system::events::Remarked>()?;
        Ok(has_event)
    }
    async fn handle_events(
        &self,
        _store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (events, block_number): (
            subxt::events::Events<TangleRuntimeConfig>,
            u64,
        ),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // find the `Remarked` event(s) in the events
        let remarked_events = events
            .find::<system::events::Remarked>()
            .flatten()
            .collect::<Vec<_>>();
        tracing::debug!(
            "Received `Remarked` Event: {:?} at block number: #{}",
            remarked_events,
            block_number
        );
        Ok(())
    }
}

#[tokio::test]
#[tracing_test::traced_test]
#[ignore = "need to be run manually"]
async fn substrate_event_watcher_should_work() -> webb_relayer_utils::Result<()>
{
    let chain_id = 5u32;
    let store = SledStore::temporary()?;
    let watcher = TestEventsWatcher::default();
    let config = webb_relayer_config::WebbRelayerConfig::default();
    let ctx = RelayerContext::new(config, store.clone())?;
    let metrics = ctx.metrics.clone();
    let event_watcher_config = EventsWatcherConfig::default();
    watcher
        .run(
            chain_id,
            ctx,
            Arc::new(store),
            event_watcher_config,
            vec![Box::<RemarkedEventHandler>::default()],
            metrics,
        )
        .await?;
    Ok(())
}
