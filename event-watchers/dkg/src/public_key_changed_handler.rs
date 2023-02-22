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

use ethereum_types::U256;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::dkg_runtime::api::dkg;
use webb::substrate::subxt::{self, OnlineClient, PolkadotConfig};

use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey, QueueStore};
use webb_relayer_utils::metric;

use webb_event_watcher_traits::substrate::EventHandler;

/// DKGPublicKeyChanged handler handles the `PublicKeySignatureChanged` event and then signals
/// signature bridge watcher to update governor.
#[derive(Clone, Debug)]
pub struct DKGPublicKeyChangedHandler {
    webb_config: webb_relayer_config::WebbRelayerConfig,
}

impl DKGPublicKeyChangedHandler {
    pub fn new(webb_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl EventHandler<PolkadotConfig> for DKGPublicKeyChangedHandler {
    type Client = OnlineClient<PolkadotConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<PolkadotConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event =
            events.has::<dkg::events::PublicKeySignatureChanged>()?;
        Ok(has_event)
    }

    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (events, block_number): (subxt::events::Events<PolkadotConfig>, u64),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // we got that the signature of the DKG public key changed.
        // that means the DKG Public Key itself changed
        let pub_key_changed_events = events
            .find::<dkg::events::PublicKeySignatureChanged>()
            .flatten()
            .collect::<Vec<_>>();
        for event in pub_key_changed_events {
            let event_details = event.clone();
            let public_key_uncompressed = event_details.uncompressed_pub_key;
            let nonce = event_details.nonce;
            tracing::debug!(
                public_key_uncompressed = %hex::encode(&public_key_uncompressed),
                %nonce,
                %block_number,
                "DKG Public Key Changed",
            );
            let bridge_keys = self
                .webb_config
                .evm
                .values()
                .map(|c| BridgeKey::new(U256::from(c.chain_id)));
            // now we just signal every signature bridge to transfer the ownership.
            for bridge_key in bridge_keys {
                tracing::debug!(
                    %bridge_key,
                    ?event,
                    "Signaling Signature Bridge to transfer ownership",
                );
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::SigningBackend,
                    backend = "DKG",
                    signal_bridge = %bridge_key,
                    public_key = %hex::encode(&public_key_uncompressed),
                    nonce = %nonce,
                    signature = %hex::encode(&event.pub_key_sig),
                );
                store.enqueue_item(
                    SledQueueKey::from_bridge_key(bridge_key),
                    BridgeCommand::TransferOwnershipWithSignature {
                        public_key: public_key_uncompressed.clone(),
                        nonce,
                        signature: event.pub_key_sig.clone(),
                    },
                )?;
            }
        }
        Ok(())
    }
}
