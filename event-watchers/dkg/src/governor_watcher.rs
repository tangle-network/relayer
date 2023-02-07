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
use webb::substrate::dkg_runtime::{self, api::dkg};
use webb::substrate::subxt::{self, OnlineClient};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey, QueueStore};
use webb_relayer_utils::metric;

use webb_event_watcher_traits::substrate::BlockNumberOf;

/// A DKG Governor watcher for the DKG Substrate runtime.
/// It watches for the DKG Public Key changes and try to update the signature bridge governor.
#[derive(Clone, Debug)]
pub struct DKGGovernorWatcher {
    webb_config: webb_relayer_config::WebbRelayerConfig,
}

impl DKGGovernorWatcher {
    pub fn new(webb_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl SubstrateEventWatcher for DKGGovernorWatcher {
    const TAG: &'static str = "DKG Governor Watcher";

    const PALLET_NAME: &'static str = "DKG";

    type RuntimeConfig = subxt::PolkadotConfig;

    type Client = OnlineClient<Self::RuntimeConfig>;

    type Event = dkg_runtime::api::Event;

    // when the DKG public key signature changes, we know the DKG is changed.
    type FilteredEvent = dkg::events::PublicKeySignatureChanged;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        _api: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // we got that the signature of the DKG public key changed.
        // that means the DKG Public Key itself changed
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
        Ok(())
    }
}
