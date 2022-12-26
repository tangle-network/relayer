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

use ethereum_types::H256;
use std::sync::Arc;
use webb::evm::contract::protocol_solidity::VAnchorContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposal_signing_backends::{
    proposal_handler, ProposalSigningBackend,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_store::EventHashStore;
use webb_relayer_store::SledStore;
use webb_relayer_utils::metric;

use crate::{HttpProvider, VAnchorContractWrapper};

/// Represents an VAnchor Contract Watcher which will use a configured signing backend for signing proposals.
pub struct VAnchorDepositHandler<B> {
    proposal_signing_backend: B,
}

impl<B> VAnchorDepositHandler<B>
where
    B: ProposalSigningBackend,
{
    pub fn new(proposal_signing_backend: B) -> Self {
        Self {
            proposal_signing_backend,
        }
    }
}

#[async_trait::async_trait]
impl<B> EventHandler for VAnchorDepositHandler<B>
where
    B: ProposalSigningBackend + Send + Sync,
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
        metrics: Arc<metric::Metrics>,
    ) -> webb_relayer_utils::Result<()> {
        use VAnchorContractEvents::*;
        let event_data = match event {
            NewCommitmentFilter(data) => {
                let chain_id = wrapper.contract.client().get_chainid().await?;
                let commitment: [u8; 32] = data.commitment.into();
                let info = (data.leaf_index.as_u32(), H256::from(commitment));
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::MerkleTreeInsertion,
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

        if event_data.leaf_index.as_u32() % 2 == 0 {
            tracing::debug!(
                leaf_index = %event_data.leaf_index,
                is_even_index = %event_data.leaf_index.as_u32() % 2 == 0,
                "VAnchor new leaf index does not satisfy the condition, skipping proposal.",
            );
            return Ok(());
        }

        let client = wrapper.contract.client();
        let chain_id = client.get_chainid().await?;
        let root: [u8; 32] =
            wrapper.contract.get_last_root().call().await?.into();
        let leaf_index = event_data.leaf_index.as_u32();
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

        for linked_anchor in linked_anchors {
            let target_resource_id = match linked_anchor {
                LinkedAnchorConfig::Raw(target) => {
                    let bytes: [u8; 32] = target.resource_id.into();
                    webb_proposals::ResourceId::from(bytes)
                }
                _ => unreachable!("unsupported"),
            };
            // Anchor update proposal proposed metric
            metrics.anchor_update_proposals.inc();
            let _ = match target_resource_id.target_system() {
                webb_proposals::TargetSystem::ContractAddress(_) => {
                    let proposal = proposal_handler::evm_anchor_update_proposal(
                        root,
                        leaf_index,
                        target_resource_id,
                        src_resource_id,
                    );
                    proposal_handler::handle_proposal(
                        &proposal,
                        &self.proposal_signing_backend,
                        metrics.clone(),
                    )
                    .await
                }
                webb_proposals::TargetSystem::Substrate(_) => {
                    let proposal =
                        proposal_handler::substrate_anchor_update_proposal(
                            root,
                            leaf_index,
                            target_resource_id,
                            src_resource_id,
                        );
                    proposal_handler::handle_proposal(
                        &proposal,
                        &self.proposal_signing_backend,
                        metrics.clone(),
                    )
                    .await
                }
            };
        }
        // mark this event as processed.
        let events_bytes = serde_json::to_vec(&event_data)?;
        store.store_event(&events_bytes)?;
        metrics.total_transaction_made.inc();
        Ok(())
    }
}
