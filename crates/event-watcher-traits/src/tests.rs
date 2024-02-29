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
    let ctx = RelayerContext::new(config, store.clone()).await?;
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
