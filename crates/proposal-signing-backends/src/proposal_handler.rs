// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use crate::ProposalSigningBackend;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::variable_anchor::v_anchor_contract;
use webb::evm::ethers::prelude::EthCall;
use webb_proposals::ProposalTrait;
use webb_relayer_utils::metric;

#[tracing::instrument(skip_all)]
pub async fn handle_proposal<PB>(
    proposal: &(impl ProposalTrait + Sync + Send + 'static),
    proposal_signing_backend: &PB,
    metrics: Arc<Mutex<metric::Metrics>>,
) -> webb_relayer_utils::Result<()>
where
    PB: ProposalSigningBackend,
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
            "the proposal is not supported by the signing backend"
        );
    }
    Ok(())
}

/// create anchor update proposal for Evm target system
#[tracing::instrument(
    skip_all,
    fields(
        proposal_type = "AnchorUpdateProposal",
        from = ?src_resource_id.typed_chain_id(),
        to = ?target_resource_id.typed_chain_id(),
        nonce = leaf_index,
        merkle_root = hex::encode(merkle_root),
    )
)]
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
    tracing::debug!("created anchor update proposal");
    webb_proposals::evm::AnchorUpdateProposal::new(
        header,
        merkle_root,
        src_resource_id,
    )
}
