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

use crate::ProposalSigningBackend;
use std::sync::Arc;
use webb::evm::contract::protocol_solidity::v_anchor_contract;
use webb::evm::ethers::prelude::EthCall;
use webb_proposals::ProposalTrait;
use webb_relayer_utils::metric;

pub async fn handle_proposal<P>(
    proposal: &(impl ProposalTrait + Sync + Send + 'static),
    proposal_signing_backend: &P,
    metrics: Arc<metric::Metrics>,
) -> webb_relayer_utils::Result<()>
where
    P: ProposalSigningBackend,
{
    let can_sign_proposal = proposal_signing_backend
        .can_handle_proposal(proposal)
        .await?;
    if can_sign_proposal {
        proposal_signing_backend
            .handle_proposal(proposal, metrics)
            .await?;
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
    let function_signature_bytes =
        v_anchor_contract::UpdateEdgeCall::selector().to_vec();
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&function_signature_bytes);
    let function_signature = webb_proposals::FunctionSignature::from(buf);
    let nonce = leaf_index;
    let header = webb_proposals::ProposalHeader::new(
        target_resource_id,
        function_signature,
        nonce.into(),
    );
    webb_proposals::evm::AnchorUpdateProposal::new(
        header,
        merkle_root,
        src_resource_id,
    )
}

// create anchor update proposal for substrate system
pub fn substrate_anchor_update_proposal(
    merkle_root: [u8; 32],
    leaf_index: u32,
    target_resource_id: webb_proposals::ResourceId,
    src_resource_id: webb_proposals::ResourceId,
) -> webb_proposals::substrate::AnchorUpdateProposal {
    let nonce = webb_proposals::Nonce::new(leaf_index);
    let function_signature =
        webb_proposals::FunctionSignature::new([0, 0, 0, 1]);
    let header = webb_proposals::ProposalHeader::new(
        target_resource_id,
        function_signature,
        nonce,
    );
    // create anchor update proposal
    webb_proposals::substrate::AnchorUpdateProposal::builder()
        .header(header)
        .merkle_root(merkle_root)
        .src_resource_id(src_resource_id)
        .build()
}
