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
use super::{HttpProvider, VAnchorContractWrapper};
use crate::proposal_signing_backend::ProposalSigningBackend;
use crate::store::sled::SledStore;
use crate::store::EventHashStore;
use ethereum_types::H256;
use std::sync::Arc;
use webb::evm::contract::protocol_solidity::{
    v_anchor_contract, VAnchorContractEvents,
};
use webb::evm::ethers::prelude::{EthCall, LogMeta, Middleware};
use webb_proposals::evm::AnchorUpdateProposal;
use webb_proposals::FunctionSignature;

/// Represents an VAnchor Contract Watcher which will use a configured signing backend for signing proposals.
pub struct VAnchorDepositHandler<B> {
    proposal_signing_backend: B,
}

impl<B> VAnchorDepositHandler<B>
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
impl<B> super::EventHandler for VAnchorDepositHandler<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal> + Send + Sync,
{
    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> crate::Result<()> {
        use VAnchorContractEvents::*;
        let event_data = match event {
            NewCommitmentFilter(data) => {
                let chain_id = wrapper.contract.client().get_chainid().await?;
                let info =
                    (data.index.as_u32(), H256::from_slice(&data.commitment));
                tracing::event!(
                    target: crate::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %crate::probe::Kind::MerkleTreeInsertion,
                    leaf_index = %info.0,
                    leaf = %info.1,
                    chain_id = %chain_id,
                    block_number = %log.block_number
                );
                data
            }
            _ => return Ok(()),
        };
        // Only construct the `AnchorUpdateProposal` if this condition evaluates to `true`: `leaf_index % 2 != 0`
        // The reason behind this is that `VAnchor` on every `transact` call, emits two events,
        // similar to the `Deposit` event but we call it the `Insertion` event, a la two `UTXO`
        // and since we only need to update the target `VAnchor` only when needed,
        // the first `Insertion` event sounds redundant in this case.
        tracing::debug!(
            event = ?event_data,
            "VAnchor new leaf event",
        );

        if event_data.index.as_u32() % 2 == 0 {
            tracing::debug!(
                leaf_index = %event_data.index,
                is_even_index = %event_data.index.as_u32() % 2 == 0,
                "VAnchor new leaf index does not satisfy the condition, skipping proposal.",
            );
            return Ok(());
        }

        let client = wrapper.contract.client();
        let chain_id = client.get_chainid().await?;
        let root = wrapper.contract.get_last_root().call().await?;
        let leaf_index = event_data.index.as_u32();
        let function_signature_bytes =
            v_anchor_contract::UpdateEdgeCall::selector().to_vec();
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&function_signature_bytes);
        let function_signature = FunctionSignature::from(buf);
        let nonce = leaf_index;
        let src_chain_id = webb_proposals::TypedChainId::Evm(chain_id.as_u32());
        let src_target_system =
            webb_proposals::TargetSystem::new_contract_address(
                wrapper.contract.address().to_fixed_bytes(),
            );
        let src_resource_id =
            webb_proposals::ResourceId::new(src_target_system, src_chain_id);

        let linked_anchors = match &wrapper.config.linked_anchors {
            Some(anchors) => anchors,
            None => {
                tracing::error!(
                    "Linked anchors not configured for : ({})",
                    chain_id
                );
                return Ok(());
            }
        };
        let linked_anchor_display = linked_anchors
            .iter()
            .map(|v| format!("Chain Id: {} at {}", v.chain_id, v.address))
            .collect::<Vec<_>>();
        tracing::debug!(
            len = linked_anchors.len(),
            anchors = ?linked_anchor_display,
            "Updating Linked Anchors"
        );
        for linked_anchor in linked_anchors {
            let dest_chain = &linked_anchor.chain_id;
            let maybe_chain = wrapper.webb_config.evm.get(dest_chain);
            let dest_chain = match maybe_chain {
                Some(chain) => chain,
                None => {
                    tracing::warn!(
                        %dest_chain,
                        chain_name = %linked_anchor.chain,
                        "Chain Id: {dest_chain} not found in the config, skipping ...",
                    );
                    continue;
                }
            };
            let anchor_target_system =
                webb_proposals::TargetSystem::new_contract_address(
                    linked_anchor.address.to_fixed_bytes(),
                );
            let anchor_chain_id =
                webb_proposals::TypedChainId::Evm(dest_chain.chain_id as _);
            let resource_id = webb_proposals::ResourceId::new(
                anchor_target_system,
                anchor_chain_id,
            );
            let header = webb_proposals::ProposalHeader::new(
                resource_id,
                function_signature,
                nonce.into(),
            );
            let proposal = webb_proposals::evm::AnchorUpdateProposal::new(
                header,
                root,
                src_resource_id,
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
                    ?proposal,
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
