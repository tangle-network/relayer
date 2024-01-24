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
use webb::substrate::subxt::{self, OnlineClient};
use webb::substrate::tangle_runtime::api::jobs::events::JobResultSubmitted;
use webb_relayer_store::queue::{QueueItem, QueueStore};
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey};
use webb_relayer_utils::{metric, TangleRuntimeConfig};

use webb_event_watcher_traits::substrate::EventHandler;

/// DKGPublicKeyChanged handler handles the `PublicKeySignatureChanged` event and then signals
/// signature bridge watcher to update governor.
#[derive(Clone, Debug)]
pub struct JobResultHandler {
    webb_config: webb_relayer_config::WebbRelayerConfig,
}

impl JobResultHandler {
    pub fn new(webb_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl EventHandler<TangleRuntimeConfig> for JobResultHandler {
    type Client = OnlineClient<TangleRuntimeConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<TangleRuntimeConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.has::<JobResultSubmitted>()?;
        Ok(has_event)
    }

    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (events, block_number): (
            subxt::events::Events<TangleRuntimeConfig>,
            u64,
        ),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // we got that the signature of the DKG public key changed.
        // that means the DKG Public Key itself changed
        let pub_key_changed_events = events
            .find::<JobResultSubmitted>()
            .flatten()
            .collect::<Vec<_>>();
        for event in pub_key_changed_events {
            let mut bridge_keys = Vec::new();
            // get evm bridges
            for (_, config) in self.webb_config.evm.iter() {
                let typed_chain_id =
                    webb_proposals::TypedChainId::Evm(config.chain_id);
                let bridge_key = BridgeKey::new(typed_chain_id);
                bridge_keys.push(bridge_key);
            }
            // get substrate bridges
            for (_, config) in self.webb_config.substrate.iter() {
                let typed_chain_id =
                    webb_proposals::TypedChainId::Substrate(config.chain_id);
                let bridge_key = BridgeKey::new(typed_chain_id);
                bridge_keys.push(bridge_key);
            }

            // now we just signal every signature bridge to transfer the ownership.
            for bridge_key in bridge_keys {
                tracing::debug!(
                    %bridge_key,
                    ?event,
                    "Signaling Signature Bridge to transfer ownership",
                );

                // enqueue transfer ownership item into store
            }
        }
        Ok(())
    }
}
