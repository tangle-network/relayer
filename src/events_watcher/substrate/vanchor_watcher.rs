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
use crate::config::LinkedAnchorConfig;
use crate::events_watcher::proposal_handler;
use crate::proposal_signing_backend::ProposalSigningBackend;
use crate::store::sled::SledStore;
use crate::store::EventHashStore;
use std::sync::Arc;
use webb::substrate::protocol_substrate_runtime::api::v_anchor_bn254;
use webb::substrate::scale::Encode;
use webb::substrate::{protocol_substrate_runtime, subxt};
use crate::metric;

/// Represents an Anchor Watcher which will use a configured signing backend for signing proposals.
pub struct SubstrateVAnchorWatcher<B> {
    proposal_signing_backend: B,
    linked_anchors: Vec<LinkedAnchorConfig>,
}

impl<B> SubstrateVAnchorWatcher<B>
where
    B: ProposalSigningBackend,
{
    pub fn new(
        proposal_signing_backend: B,
        linked_anchors: Vec<LinkedAnchorConfig>,
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
    B: ProposalSigningBackend + Send + Sync,
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
        metrics: Arc<metric::Metrics>,
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
        for linked_anchor in &self.linked_anchors {
            let target_resource_id = match linked_anchor {
                LinkedAnchorConfig::Raw(target) => {
                    let bytes: [u8; 32] = target.resource_id.into();
                    webb_proposals::ResourceId::from(bytes)
                }
                _ => unreachable!("unsupported"),
            };

            let _ = match target_resource_id.target_system() {
                webb_proposals::TargetSystem::ContractAddress(_) => {
                    let proposal = proposal_handler::evm_anchor_update_proposal(
                        merkle_root,
                        latest_leaf_index,
                        target_resource_id,
                        src_resource_id,
                    );
                    proposal_handler::handle_proposal(
                        &proposal,
                        &self.proposal_signing_backend,
                        metrics.clone()
                    )
                    .await
                }
                webb_proposals::TargetSystem::Substrate(_) => {
                    let proposal =
                        proposal_handler::substrate_anchor_update_propsoal(
                            merkle_root,
                            latest_leaf_index,
                            target_resource_id,
                            src_resource_id,
                        );
                    proposal_handler::handle_proposal(
                        &proposal,
                        &self.proposal_signing_backend,
                        metrics.clone()
                    )
                    .await
                }
            };
        }
        // mark this event as processed.
        let events_bytes = &event.encode();
        store.store_event(events_bytes)?;
        Ok(())
    }
}
