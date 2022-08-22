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
use super::BlockNumberOf;
use crate::config::SubstrateLinkedVAnchorConfig;
use crate::proposal_signing_backend::ProposalSigningBackend;
use crate::store::sled::SledStore;
use crate::store::EventHashStore;
use std::sync::Arc;
use webb::substrate::protocol_substrate_runtime::api::v_anchor_bn254;
use webb::substrate::scale::Encode;
use webb::substrate::{protocol_substrate_runtime, subxt};
use webb_proposals::substrate::AnchorUpdateProposal;

/// Represents an Anchor Watcher which will use a configured signing backend for signing proposals.
pub struct SubstrateVAnchorWatcher<B> {
    proposal_signing_backend: B,
    linked_anchors: Vec<SubstrateLinkedVAnchorConfig>,
}

impl<B> SubstrateVAnchorWatcher<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal>,
{
    pub fn new(
        proposal_signing_backend: B,
        linked_anchors: Vec<SubstrateLinkedVAnchorConfig>,
    ) -> Self {
        Self {
            proposal_signing_backend,
            linked_anchors,
        }
    }
}

#[async_trait::async_trait]
impl<B> super::SubstrateEventWatcher for SubstrateVAnchorWatcher<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal> + Send + Sync,
{
    const TAG: &'static str = "Substrate V-Anchor Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = protocol_substrate_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::SubstrateExtrinsicParams<Self::RuntimeConfig>,
    >;

    type Event = protocol_substrate_runtime::api::Event;

    type FilteredEvent = v_anchor_bn254::events::Transaction;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
    ) -> crate::Result<()> {
        tracing::debug!(
            event = ?event,
            "V-Anchor new leaf event",
        );
        let chain_id =
            api.constants().linkable_tree_bn254().chain_identifier()?;

        let at_hash = api
            .storage()
            .system()
            .block_hash(&u64::from(block_number), None)
            .await?;
        // fetch tree
        let tree = api
            .storage()
            .merkle_tree_bn254()
            .trees(&event.tree_id, Some(at_hash))
            .await?;
        let tree = match tree {
            Some(t) => t,
            None => return Err(crate::Error::Generic("No tree found")),
        };
        // pallet index
        let pallet_index = {
            let locked_metadata = api.client.metadata();
            let metadata = locked_metadata.read();
            let pallet = metadata.pallet("VAnchorHandlerBn254")?;
            pallet.index()
        };

        let root = tree.root;
        let latest_leaf_index = tree.leaf_count;
        let tree_id = event.tree_id;
        let nonce = webb_proposals::Nonce::new(latest_leaf_index);
        let function_signature =
            webb_proposals::FunctionSignature::new([0, 0, 0, 2]);
        let src_chain_id =
            webb_proposals::TypedChainId::Substrate(chain_id as u32);
        let target = webb_proposals::SubstrateTargetSystem::builder()
            .pallet_index(pallet_index)
            .tree_id(tree_id)
            .build();
        let src_target_system = webb_proposals::TargetSystem::Substrate(target);
        let src_resource_id =
            webb_proposals::ResourceId::new(src_target_system, src_chain_id);
        let mut merkle_root = [0; 32];
        merkle_root.copy_from_slice(&root.encode());

        // update linked anchors
        for anchor in &self.linked_anchors {
            let anchor_chain_id =
                webb_proposals::TypedChainId::Substrate(anchor.chain);
            let target = webb_proposals::SubstrateTargetSystem::builder()
                .pallet_index(pallet_index)
                .tree_id(anchor.tree)
                .build();
            let anchor_target_system =
                webb_proposals::TargetSystem::Substrate(target);
            let resource_id = webb_proposals::ResourceId::new(
                anchor_target_system,
                anchor_chain_id,
            );
            let header = webb_proposals::ProposalHeader::new(
                resource_id,
                function_signature,
                nonce,
            );

            // create anchor update proposal
            let proposal = AnchorUpdateProposal::builder()
                .header(header)
                .src_resource_id(src_resource_id)
                .merkle_root(merkle_root)
                .build();

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
                    "V-Anchor update proposal is not supported by the signing backend"
                );
            }
        }
        // mark this event as processed.
        let events_bytes = &event.encode();
        store.store_event(events_bytes)?;
        Ok(())
    }
}
