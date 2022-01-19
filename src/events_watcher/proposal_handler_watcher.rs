use std::sync::Arc;

use webb::evm::contract::protocol_solidity::AnchorHandlerContract;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;
use webb::substrate::{dkg_runtime, subxt};

use crate::context::RelayerContext;
use crate::events_watcher::{
    decode_resource_id, BridgeKey, ProposalDataWithSignature, ProposalHeader,
    SignatureBridgeCommand,
};
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::QueueStore;

use super::{BlockNumberOf, SubstrateEventWatcher};

pub struct ProposalHandlerWatcher {
    ctx: RelayerContext,
}

impl ProposalHandlerWatcher {
    pub fn new(ctx: RelayerContext) -> Self {
        Self { ctx }
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
        tracing::debug!(
            "Received `ProposalSigned` Event: {:?} at block number: #{}",
            event,
            block_number
        );
        // now we need to signal signature bridge.
        // the way we are going to get the signature bridge contract address
        // is simply through these steps:
        // 1. Decode the Proposal Header.
        // 2. extract the anchor handler address from the resource id.
        // 3. connect to the anchor handler address
        // 4. get the bridge address from the anchor handler.
        // 5. finally, create the Bridge Key.
        let header = ProposalHeader::decode(&event.data)?;
        let (anchor_handler_address, chain_id) =
            decode_resource_id(header.resource_id);
        let client = self.ctx.evm_provider_by_chain_id(chain_id).await?;
        let anchor_handler_contract = AnchorHandlerContract::new(
            anchor_handler_address,
            Arc::new(client),
        );
        tracing::debug!(%anchor_handler_address, %chain_id, "Decoded Resource Id");
        let bridge_address =
            anchor_handler_contract.bridge_address().call().await?;
        tracing::debug!(%bridge_address, "Got Bridge Address from the Anchor Handler");

        let bridge_key = BridgeKey::new(bridge_address, chain_id);
        // and now we can signal the bridge.
        tracing::debug!(
            %bridge_key,
            "Signal the Signatrue Bridge with new Signed Proposal"
        );
        store.enqueue_item(
            SledQueueKey::from_bridge_key(bridge_key),
            SignatureBridgeCommand::ExecuteProposal(
                ProposalDataWithSignature {
                    data: event.data,
                    signature: event.signature,
                },
            ),
        )?;
        Ok(())
    }
}
