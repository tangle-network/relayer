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

use webb::substrate::dkg_runtime::api::dkg_proposal_handler;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::{dkg_runtime, subxt};

use crate::config::{self, Contract};
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::{BridgeCommand, BridgeKey, QueueStore};

use super::{BlockNumberOf, SubstrateEventWatcher};

/// A ProposalHandler watcher for the DKG Substrate runtime.
/// It watches for the `ProposalSigned` event and sends the proposal to the signature bridge.
#[derive(Clone, Debug)]
pub struct ProposalHandlerWatcher {
    webb_config: config::WebbRelayerConfig,
}

impl ProposalHandlerWatcher {
    pub fn new(webb_config: config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl SubstrateEventWatcher for ProposalHandlerWatcher {
    const TAG: &'static str = "DKG Signed Proposal Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = dkg_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;

    type Event = dkg_proposal_handler::events::ProposalSigned;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        _api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SigningBackend,
            backend = "DKG",
            ty = "ProposalSigned",
            ?event.target_chain,
            ?event.key,
            ?block_number,
        );
        let maybe_bridge_key = match event.target_chain {
            TypedChainId::None => {
                tracing::warn!(
                    "Received `ProposalSigned` Event with no chain id"
                );
                None
            }
            TypedChainId::Evm(id) => self
                .webb_config
                .evm
                .values()
                .find(|c| c.chain_id == u64::from(id))
                .and_then(|c| {
                    c.contracts.iter().find(|contract| {
                        matches!(contract, Contract::SignatureBridge(_))
                    })
                })
                .and_then(|contract| match contract {
                    Contract::SignatureBridge(bridge) => Some(bridge),
                    _ => None,
                })
                .map(|config| {
                    BridgeKey::new(
                        config.common.address,
                        webb_proposals::TypedChainId::Evm(id),
                    )
                }),
            TypedChainId::Substrate(_) => {
                tracing::warn!(
                    "Unhandled `ProposalSigned` Event with substrate chain id"
                );
                None
            }
            TypedChainId::PolkadotParachain(_) => {
                tracing::warn!("Unhandled `ProposalSigned` Event with polkadot parachain chain id");
                None
            }
            TypedChainId::KusamaParachain(_) => {
                tracing::warn!("Unhandled `ProposalSigned` Event with kusama parachain chain id");
                None
            }
            TypedChainId::RococoParachain(_) => {
                tracing::warn!("Unhandled `ProposalSigned` Event with rococo parachain chain id");
                None
            }
            TypedChainId::Cosmos(_) => {
                tracing::warn!(
                    "Unhandled `ProposalSigned` Event with cosmos chain id"
                );
                None
            }
            TypedChainId::Solana(_) => {
                tracing::warn!(
                    "Unhandled `ProposalSigned` Event with solana chain id"
                );
                None
            }
        };
        tracing::debug!(?maybe_bridge_key, "Sending Proposal to the bridge");
        // now we just signal the bridge with the proposal.
        let bridge_key = match maybe_bridge_key {
            Some(bridge_key) => bridge_key,
            None => {
                tracing::warn!(
                    ?event.target_chain,
                    "No bridge configured for that chain, skipping",
                );
                return Ok(());
            }
        };
        tracing::debug!(
            %bridge_key,
            proposal = ?event,
            "Signaling Signature Bridge to execute proposal",
        );
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SigningBackend,
            backend = "DKG",
            signal_bridge = %bridge_key,
            data = ?hex::encode(&event.data),
            signature = ?hex::encode(&event.signature),
        );
        store.enqueue_item(
            SledQueueKey::from_bridge_key(bridge_key),
            BridgeCommand::ExecuteProposalWithSignature {
                data: event.data.clone(),
                signature: event.signature,
            },
        )?;
        Ok(())
    }
}
