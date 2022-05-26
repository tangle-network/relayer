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
use super::{BlockNumberOf, SubstrateEventWatcher};
use crate::events_watcher::proposal_signing_backend::ProposalSigningBackend;
use crate::events_watcher::SubstrateBridgeWatcher;
use crate::store::sled::SledStore;
use crate::store::EventHashStore;
use std::convert::TryInto;
use std::sync::Arc;
use webb::substrate::protocol_substrate_runtime::api::anchor_bn254;
use webb::substrate::scale::Encode;
use webb::substrate::{protocol_substrate_runtime, subxt};
use webb_proposals::substrate::AnchorUpdateProposal;

/// Represents an Anchor Contract Watcher which will use a configured signing backend for signing proposals.
pub struct SubstrateAnchorWatcher<B> {
    proposal_signing_backend: B,
}

impl<B> SubstrateAnchorWatcher<B>
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
impl<B> super::SubstrateEventWatcher for SubstrateAnchorWatcher<B>
where
    B: ProposalSigningBackend<AnchorUpdateProposal> + Send + Sync,
{
    const TAG: &'static str = "Substrate Anchor Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = protocol_substrate_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;

    type Event = anchor_bn254::events::Deposit;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        tracing::debug!("Substrate Anchor deposit event",);
        // fetch chain_id
        let chain_id =
            api.constants().linkable_tree_bn254().chain_identifier()?;

        let at_hash = api
            .storage()
            .system()
            .block_hash(block_number, None)
            .await?;

        let tree = api
            .storage()
            .merkle_tree_bn254()
            .trees(event.tree_id, Some(at_hash))
            .await?;
        let edges_list = api
            .storage()
            .linkable_tree_bn254()
            .edge_list(
                chain_id.try_into().unwrap(),
                event.tree_id.try_into().unwrap(),
                Some(at_hash),
            )
            .await?;
        // .edge_list(chain_id.try_into().unwrap(), event.tree_id.try_into().unwrap(), Some(at_hash))
        tracing::debug!(
            "####SUBSTRATE ANCHOR WACTHER MERKELE EDGELIST {:?}  TREE : {:?}",
            edges_list,
            event.tree_id
        );

        let tree = match tree {
            Some(t) => t,
            None => return Err(anyhow::anyhow!("anchor not found")),
        };

        let root = tree.root;
        let latest_leaf_index = tree.leaf_count;
        let tree_id = event.tree_id;

        let target_system = webb_proposals::TargetSystem::new_tree_id(tree_id);
        let src_chain =
            webb_proposals::TypedChainId::Substrate(chain_id as u32);
        let resource_id =
            webb_proposals::ResourceId::new(target_system, src_chain);
        let nonce =
            webb_proposals::Nonce::new(latest_leaf_index.try_into().unwrap());
        let function_signature = webb_proposals::FunctionSignature::new([0; 4]);
        let header = webb_proposals::ProposalHeader::new(
            resource_id,
            function_signature,
            nonce,
        );

        let mut merkle_root = [0; 32];
        merkle_root.copy_from_slice(&root.encode());

        // create anchor update proposal
        let anchor_update_proposal = AnchorUpdateProposal::builder()
            .header(header)
            .src_chain(src_chain)
            .merkle_root(merkle_root)
            .latest_leaf_index(latest_leaf_index)
            .target(target_system.into_fixed_bytes())
            .pallet_index(10)
            .build();

        // edges =

        // for edge in edges {
        //     // first, we get the target chain tree id
        // 	let other_chain_id = edge.src_chain_id;
        // 	let target_tree_id = LinkedAnchors::<T, I>::get(other_chain_id, tree_id);
        // 	let my_chain_id = src_chain_id;

        // 	let other_chain_underlying_chain_id: u32 =
        // 		utils::get_underlying_chain_id(other_chain_id.try_into().unwrap_or_default());
        // 	let tree: u32 = target_tree_id.try_into().unwrap_or_default();

        // 	let r_id = utils::derive_resource_id(
        // 		other_chain_underlying_chain_id,
        // 		target_tree_id.try_into().unwrap_or_default(),
        // 	);

        // 	// construct the proposal header
        // 	let proposal_header = webb_proposals::ProposalHeader::new(r_id, function_signature, nonce);

        // 	// construct the anchor update proposal
        // 	let anchor_update_proposal = AnchorUpdateProposal::new(
        // 		proposal_header,
        // 		typed_src_chain_id,
        // 		latest_leaf_index_u32,
        // 		merkle_root,
        // 	);
        //     let can_sign_proposal = self
        //         .proposal_signing_backend
        //         .can_handle_proposal(&proposal)
        //         .await?;
        //     if can_sign_proposal {
        //         self.proposal_signing_backend
        //             .handle_proposal(&proposal)
        //             .await?;
        //     } else {
        //         tracing::warn!(
        //             "Anchor update proposal is not supported by the signing backend"
        //         );
        //     }
        // }
        // mark this event as processed.

        // let events_bytes = serde_json::to_vec(&event_data)?;
        // store.store_event(&events_bytes)?;
        Ok(())
    }
}
