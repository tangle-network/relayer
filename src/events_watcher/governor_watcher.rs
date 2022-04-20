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
//
use std::sync::Arc;

use webb::substrate::dkg_runtime::api::dkg;
use webb::substrate::{dkg_runtime, subxt};

use crate::config::{self, Contract};
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::{BridgeCommand, BridgeKey, QueueStore};

use super::{BlockNumberOf, SubstrateEventWatcher};

/// A DKG Governor watcher for the DKG Substrate runtime.
/// It watches for the DKG Public Key changes and try to update the signature bridge governor.
#[derive(Clone, Debug)]
pub struct DKGGovernorWatcher {
    webb_config: config::WebbRelayerConfig,
}

impl DKGGovernorWatcher {
    pub fn new(webb_config: config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl SubstrateEventWatcher for DKGGovernorWatcher {
    const TAG: &'static str = "DKG Governor Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = dkg_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;
    // when the DKG public key signature changes, we know the DKG is changed.
    type Event = dkg::events::PublicKeySignatureChanged;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // we got that the signature of the DKG public key changed.
        // that means the DKG Public Key itself changed.
        // so we need to query the public key from the storage:

        // Note: here we need to get the public key from the storage at the moment of that event.
        let at_hash = api
            .storage()
            .system()
            .block_hash(block_number, None)
            .await?;
        let (_authority_id, public_key_compressed) =
            api.storage().dkg().dkg_public_key(Some(at_hash)).await?;
        let refresh_nonce =
            api.storage().dkg().refresh_nonce(Some(at_hash)).await?;
        // next is that we need to uncompress the public key.
        let public_key_uncompressed =
            decompress_public_key(public_key_compressed)?;
        let bridge_keys = self
            .webb_config
            .evm
            .values()
            .map(|c| (c.chain_id, &c.contracts))
            .flat_map(|(chain_id, contracts)| {
                // find first signature bridge contract for this chain.
                contracts
                    .iter()
                    .find(|contract| {
                        matches!(contract, Contract::SignatureBridge(_))
                    })
                    .map(|contract| (chain_id, contract))
            })
            .flat_map(|(chain_id, contract)| match contract {
                Contract::SignatureBridge(contract_config) => {
                    Some((chain_id, contract_config.common.address))
                }
                _ => None,
            })
            .map(|(chain_id, address)| {
                BridgeKey::new(address, chain_id.into())
            });
        // now we just signal every signature bridge to transfer the ownership.
        for bridge_key in bridge_keys {
            tracing::debug!(
                %bridge_key,
                ?event,
                "Signaling Signature Bridge to transfer ownership",
            );
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::SigningBackend,
                backend = "DKG",
                signal_bridge = %bridge_key,
                public_key = ?hex::encode(&public_key_uncompressed),
                nonce = %refresh_nonce,
                signature = ?hex::encode(&event.pub_key_sig),
            );
            store.enqueue_item(
                SledQueueKey::from_bridge_key(bridge_key),
                BridgeCommand::TransferOwnershipWithSignature {
                    public_key: public_key_uncompressed.clone(),
                    nonce: refresh_nonce,
                    signature: event.pub_key_sig.clone(),
                },
            )?;
        }
        Ok(())
    }
}

/// Decompress the compressed public key and return the uncompressed public key.
/// **Note:** it also removes the 0x04 prefix, so the result is the uncompressed public key without the prefix.
pub fn decompress_public_key(compressed: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    let result = libsecp256k1::PublicKey::parse_slice(
        &compressed,
        Some(libsecp256k1::PublicKeyFormat::Compressed),
    )
    .map(|pk| pk.serialize())?;
    if result.len() == 65 {
        // remove the 0x04 prefix
        Ok(result[1..].to_vec())
    } else {
        Ok(result.to_vec())
    }
}
