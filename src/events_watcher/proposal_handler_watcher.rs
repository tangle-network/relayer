use webb::substrate::dkg_runtime;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;

use crate::store::sled::SledStore;

use super::{BlockNumberOf, SubstrateEventWatcher};

pub struct ProposalHandlerWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for ProposalHandlerWatcher {
    const TAG: &'static str = "Proposal Handler Watcher";

    type RuntimeConfig = dkg_runtime::api::DefaultConfig;

    type Api = dkg_runtime::api::RuntimeApi<Self::RuntimeConfig>;

    type Event = dkg_proposal_handler::events::ProposalAdded;

    type Store = SledStore;

    async fn handle_event(
        &self,
        _store: std::sync::Arc<Self::Store>,
        _api: std::sync::Arc<Self::Api>,
        (event, _block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // TODO(@shekohex): implement this over ProposalSigned event.
        dbg!(event.0);
        Ok(())
    }
}
