use std::sync::Arc;

use webb::substrate::dkg_runtime;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;

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
        tracing::debug!(
            "Received `ProposalSigned` Event: {:?} at block number: #{}",
            event,
            block_number
        );
        Ok(())
    }
}
