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
use webb::substrate::dkg_runtime::api::system;
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb::substrate::{dkg_runtime, subxt};
use webb_relayer_context::RelayerContext;
use webb_relayer_store::sled::SledStore;
use webb_relayer_utils::metric;

use crate::substrate::BlockNumberOf;
use crate::SubstrateEventWatcher;

#[derive(Debug, Clone, Default)]
struct RemarkedEventWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for RemarkedEventWatcher {
    const TAG: &'static str = "Remarked Event Watcher";

    const PALLET_NAME: &'static str = "System";

    type RuntimeConfig = subxt::PolkadotConfig;

    type Client = OnlineClient<Self::RuntimeConfig>;

    type Event = dkg_runtime::api::Event;
    type FilteredEvent = system::events::Remarked;

    type Store = SledStore;

    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        tracing::debug!(
            "Received `Remarked` Event: {:?} at block number: #{}",
            event,
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
    let node_name = String::from("test-node");
    let chain_id = 5u32;
    let store = Arc::new(SledStore::temporary()?);
    let client = OnlineClient::<PolkadotConfig>::new().await?;
    let watcher = RemarkedEventWatcher::default();
    let config = webb_relayer_config::WebbRelayerConfig::default();
    let ctx = RelayerContext::new(config);
    let metrics = ctx.metrics.clone();
    watcher
        .run(node_name, chain_id, client.into(), store, metrics)
        .await?;
    Ok(())
}
