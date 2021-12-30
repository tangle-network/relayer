use std::sync::Arc;

use webb::substrate::dkg_runtime;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;
use webb::substrate::dkg_runtime::api::runtime_types::dkg_runtime_primitives::proposal::DKGPayloadKey;

use crate::store::sled::SledStore;

use super::{BlockNumberOf, SubstrateEventWatcher};

pub struct ProposalHandlerWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for ProposalHandlerWatcher {
    const TAG: &'static str = "DKG Signed Proposal Watcher";

    type RuntimeConfig = dkg_runtime::api::DefaultConfig;

    type Api = dkg_runtime::api::RuntimeApi<Self::RuntimeConfig>;

    type Event = dkg_proposal_handler::events::ProposalSigned;

    type Store = SledStore;

    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // TODO: handle every type of these proposals
        let key = match event.key {
            DKGPayloadKey::EVMProposal(n) => format!("EVMProposal({})", n),
            DKGPayloadKey::RefreshVote(n) => format!("RefreshVote({})", n),
            DKGPayloadKey::AnchorUpdateProposal(n) => {
                format!("AnchorUpdateProposal({})", n)
            }
            DKGPayloadKey::TokenAddProposal(n) => {
                format!("TokenAddProposal({})", n)
            }
            DKGPayloadKey::TokenRemoveProposal(n) => {
                format!("TokenRemoveProposal({})", n)
            }
            DKGPayloadKey::WrappingFeeUpdateProposal(n) => {
                format!("WrappingFeeUpdateProposal({})", n)
            }
            DKGPayloadKey::ResourceIdUpdateProposal(n) => {
                format!("ResourceIdUpdateProposal({})", n)
            }
        };
        tracing::debug!(
            "Received `ProposalSigned` Event:
            - `chain_id`: {},
            - `key`: {},
            - `data`: 0x{},
            - `signature`: 0x{},
            at block: #{}",
            event.chain_id,
            key,
            hex::encode(&event.data),
            hex::encode(&event.signature),
            block_number
        );
        Ok(())
    }
}
