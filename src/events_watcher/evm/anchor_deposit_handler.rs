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
use super::AnchorContractWrapper;
use crate::proposal_signing_backend::ProposalSigningBackend;
use crate::store::sled::SledStore;
use crate::store::EventHashStore;
use std::sync::Arc;
use webb::evm::contract::protocol_solidity::FixedDepositAnchorContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_proposals::evm::AnchorUpdateProposal;

/// Represents an Anchor Contract Deposit Event Watcher which will use a configured signing backend for signing proposals.
///
/// This mainly watching for Deposit events which then will create [`AnchorUpdateProposal`]s and send them to the signing backend.
pub struct AnchorDepoistEventHandler<B> {
    proposal_signing_backend: B,
}

impl<B> AnchorDepoistEventHandler<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal>,
{
    pub fn new(proposal_signing_backend: B) -> Self {
        Self {
            proposal_signing_backend,
        }
    }
}

#[async_trait::async_trait]
impl<B> super::EventHandler for AnchorDepoistEventHandler<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal> + Send + Sync,
{
    type Middleware = super::HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = FixedDepositAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use FixedDepositAnchorContractEvents::*;
        // only process anchor deposit events.
        let event_data = match event {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
        tracing::debug!(
            event = ?event_data,
            "Anchor deposit event",
        );
        let client = wrapper.contract.client();
        let src_chain_id = client.get_chainid().await?;
        let root = wrapper.contract.get_last_root().call().await?;
        let leaf_index = event_data.leaf_index;
        let function_signature = [141, 9, 22, 157];
        let nonce = event_data.leaf_index;
        let linked_anchors = match &wrapper.config.linked_anchors {
            Some(anchors) => anchors,
            None => {
                tracing::warn!(
                    "Linked anchors not configured for : ({})",
                    src_chain_id
                );
                return Ok(());
            }
        };
        for linked_anchor in linked_anchors {
            let dest_chain = linked_anchor.chain.to_lowercase();
            let maybe_chain = wrapper.webb_config.evm.get(&dest_chain);
            let dest_chain = match maybe_chain {
                Some(chain) => chain,
                None => continue,
            };
            let target_system =
                webb_proposals::TargetSystem::new_contract_address(
                    linked_anchor.address.to_fixed_bytes(),
                );
            let typed_chain_id =
                webb_proposals::TypedChainId::Evm(dest_chain.chain_id as _);
            let resource_id =
                webb_proposals::ResourceId::new(target_system, typed_chain_id);
            let header = webb_proposals::ProposalHeader::new(
                resource_id,
                function_signature.into(),
                nonce.into(),
            );
            let proposal = webb_proposals::evm::AnchorUpdateProposal::new(
                header,
                webb_proposals::TypedChainId::Evm(src_chain_id.as_u32()),
                leaf_index,
                root,
                target_system.into_fixed_bytes(),
            );
            let can_sign_proposal = self
                .proposal_signing_backend
                .can_handle_proposal(&proposal)
                .await?;
            if can_sign_proposal {
                self.proposal_signing_backend
                    .handle_proposal(&proposal)
                    .await?;
            } else {
                tracing::warn!(
                    "Anchor update proposal is not supported by the signing backend"
                );
            }
        }
        // mark this event as processed.

        let events_bytes = serde_json::to_vec(&event_data)?;
        store.store_event(&events_bytes)?;
        Ok(())
    }
}
