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

use webb::substrate::dkg_runtime;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::subxt::{self, OnlineClient};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey, QueueStore};
use webb_relayer_utils::metric;

use webb_event_watcher_traits::substrate::BlockNumberOf;

/// A ProposalHandler watcher for the DKG Substrate runtime.
/// It watches for the `ProposalSigned` event and sends the proposal to the signature bridge.
#[derive(Copy, Clone, Debug, Default)]
pub struct ProposalHandlerWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for ProposalHandlerWatcher {
    const TAG: &'static str = "DKG Signed Proposal Watcher";

    type RuntimeConfig = subxt::PolkadotConfig;

    type Client = OnlineClient<Self::RuntimeConfig>;

    type Event = dkg_runtime::api::Event;

    type FilteredEvent = dkg_proposal_handler::events::ProposalSigned;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        _api: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        metrics: Arc<metric::Metrics>,
    ) -> webb_relayer_utils::Result<()> {
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SigningBackend,
            backend = "DKG",
            ty = "ProposalSigned",
            ?event.target_chain,
            ?event.key,
            ?block_number,
        );
        let maybe_bridge_key = match event.target_chain {
            TypedChainId::None => {
                tracing::debug!(
                    "Received `ProposalSigned` Event with no chain id, ignoring",
                );
                None
            }
            TypedChainId::Evm(id) => {
                tracing::trace!(
                    "`ProposalSigned` Event with evm chain id : {}",
                    id
                );
                Some(BridgeKey::new(webb_proposals::TypedChainId::Evm(id)))
            }
            TypedChainId::Substrate(id) => {
                tracing::trace!(
                    "`ProposalSigned` Event with substrate chain id : {}",
                    id
                );
                Some(BridgeKey::new(webb_proposals::TypedChainId::Substrate(
                    id,
                )))
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
            TypedChainId::Ink(_) => {
                tracing::warn!(
                    "Unhandled `ProposalSigned` Event with Ink chain id"
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
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SigningBackend,
            backend = "DKG",
            signal_bridge = %bridge_key,
            data = ?hex::encode(&event.data),
            signature = ?hex::encode(&event.signature),
        );
        // Proposal signed metric
        metrics.proposals_signed.inc();
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
