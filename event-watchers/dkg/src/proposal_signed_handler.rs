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

use webb::substrate::tangle_runtime::api::dkg_proposal_handler;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::subxt::{self, OnlineClient, PolkadotConfig};

use webb_relayer_store::queue::{QueueItem, QueueStore};
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey};
use webb_relayer_utils::metric;

use webb_event_watcher_traits::substrate::EventHandler;

/// A ProposalSignedHandler handles the `ProposalSigned` event and signals signature bridge to execute them.
#[derive(Copy, Clone, Debug, Default)]
pub struct ProposalSignedHandler;

#[async_trait::async_trait]
impl EventHandler<PolkadotConfig> for ProposalSignedHandler {
    type Client = OnlineClient<PolkadotConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<PolkadotConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events
            .has::<dkg_proposal_handler::events::ProposalBatchSigned>(
        )?;
        Ok(has_event)
    }

    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        _api: Arc<Self::Client>,
        (events, block_number): (subxt::events::Events<PolkadotConfig>, u64),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let proposal_signed_events = events
            .find::<dkg_proposal_handler::events::ProposalBatchSigned>()
            .flatten()
            .collect::<Vec<_>>();
        for event in proposal_signed_events {
            tracing::event!(
                target: webb_relayer_utils::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %webb_relayer_utils::probe::Kind::SigningBackend,
                backend = "DKG",
                ty = "ProposalBatchSigned",
                ?event.target_chain,
                ?event.batch_id,
                ?block_number,
            );
            let maybe_bridge_key = match event.target_chain {
                TypedChainId::None => {
                    tracing::debug!(
                        "Received `ProposalBatchSigned` Event with no chain id, ignoring",
                    );
                    None
                }
                TypedChainId::Evm(id) => {
                    tracing::trace!(
                        chain_id = %id,
                        "`ProposalBatchSigned` Event with evm",
                    );
                    Some(BridgeKey::new(webb_proposals::TypedChainId::Evm(id)))
                }
                TypedChainId::Substrate(id) => {
                    tracing::trace!(
                        chain_id = %id,
                        "`ProposalBatchSigned` Event with substrate",
                    );
                    Some(BridgeKey::new(
                        webb_proposals::TypedChainId::Substrate(id),
                    ))
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
            tracing::debug!(
                ?maybe_bridge_key,
                "Sending Proposal to the bridge"
            );
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
                signature = %hex::encode(&event.signature),
            );
            // Proposal signed metric
            metrics.lock().await.proposals_signed.inc();
            let item = QueueItem::new(BridgeCommand::try_from(event)?);
            store.enqueue_item(
                SledQueueKey::from_bridge_key(bridge_key),
                item,
            )?;
        }
        Ok(())
    }
}
