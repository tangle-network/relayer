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
use webb_proposals::ProposalTrait;
use crate::proposal_signing_backend::ProposalSigningBackend;
use webb::evm::contract::protocol_solidity::variable_anchor::v_anchor_contract::NewCommitmentFilter;
use webb::substrate::{protocol_substrate_runtime, subxt};
use std::sync::Arc;
use webb::substrate::scale::Encode;

pub async fn handle_proposal<P>(
    proposal: &(impl ProposalTrait + Sync + Send + 'static),
    proposal_signing_backend: &P,
) -> crate::Result<()>
where
    P: ProposalSigningBackend,
{
    let can_sign_proposal = proposal_signing_backend
        .can_handle_proposal(proposal)
        .await?;
    if can_sign_proposal {
        proposal_signing_backend.handle_proposal(proposal).await?;
    } else {
        tracing::warn!(
            proposal = ?hex::encode(proposal.to_vec()),
            "Anchor update proposal is not supported by the signing backend"
        );
    }
    Ok(())
}

// create anchor update proposal for Evm target system
pub fn evm_anchor_update_proposal(
    merkle_root: [u8; 32],
    leaf_index: u32,
    target_resource_id: webb_proposals::ResourceId,
    src_resource_id: webb_proposals::ResourceId,
) -> webb_proposals::evm::AnchorUpdateProposal {
    let function_signature = [141, 9, 22, 157];
    let nonce = leaf_index;
    let header = webb_proposals::ProposalHeader::new(
        target_resource_id,
        function_signature.into(),
        nonce.into(),
    );
    let proposal = webb_proposals::evm::AnchorUpdateProposal::new(
        header,
        merkle_root,
        src_resource_id
    );
    return proposal;
}

//
pub fn substrate_anchor_update_propsoal(
    merkle_root: [u8; 32],
    leaf_index: u32,
    target_resource_id: webb_proposals::ResourceId,
    src_resource_id: webb_proposals::ResourceId,
) -> webb_proposals::substrate::AnchorUpdateProposal {

    let nonce = webb_proposals::Nonce::new(leaf_index);
    let function_signature = webb_proposals::FunctionSignature::new([0, 0, 0, 2]);
    let header = webb_proposals::ProposalHeader::new(
        target_resource_id,
        function_signature,
        nonce,
    );
    // create anchor update proposal
    let proposal = webb_proposals::substrate::AnchorUpdateProposal::builder()
        .header(header)
        .merkle_root(merkle_root)
        .src_resource_id(src_resource_id)
        .build();
    return proposal;
}
