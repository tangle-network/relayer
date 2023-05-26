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

use std::sync::Arc;
use tokio::sync::Mutex;

use webb::substrate::scale::Encode;
use webb::substrate::subxt::{self, OnlineClient, PolkadotConfig};
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::tangle_runtime::api::v_anchor_bn254;
use webb_bridge_registry_backends::BridgeRegistryBackend;
use webb_event_watcher_traits::substrate::EventHandler;

use webb_proposal_signing_backends::{
    proposal_handler, ProposalSigningBackend,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_store::EventHashStore;
use webb_relayer_store::SledStore;
use webb_relayer_utils::metric;
/// SubstrateVAnchorDeposit handler handles `Transaction` event and creates `AnchorUpdate` proposals for linked anchors.
pub struct SubstrateVAnchorDepositHandler<B, C> {
    proposal_signing_backend: B,
    bridge_registry_backend: C,
    linked_anchors: Option<Vec<LinkedAnchorConfig>>,
}

impl<B, C> SubstrateVAnchorDepositHandler<B, C>
where
    B: ProposalSigningBackend,
    C: BridgeRegistryBackend,
{
    pub fn new(
        proposal_signing_backend: B,
        bridge_registry_backend: C,
        linked_anchors: Option<Vec<LinkedAnchorConfig>>,
    ) -> Self {
        Self {
            proposal_signing_backend,
            bridge_registry_backend,
            linked_anchors,
        }
    }
}

#[async_trait::async_trait]
impl<B, C> EventHandler<PolkadotConfig> for SubstrateVAnchorDepositHandler<B, C>
where
    B: ProposalSigningBackend + Send + Sync,
    C: BridgeRegistryBackend + Send + Sync,
{
    type Client = OnlineClient<PolkadotConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<PolkadotConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.has::<v_anchor_bn254::events::Transaction>()?;
        Ok(has_event)
    }

    #[tracing::instrument(skip_all)]
    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (events, _block_number): (subxt::events::Events<PolkadotConfig>, u64),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let at_hash = events.block_hash();
        let metrics_clone = metrics.clone();
        // fetch chain_id
        let chain_id_addrs = RuntimeApi::constants()
            .linkable_tree_bn254()
            .chain_identifier();
        let chain_id = client.constants().at(&chain_id_addrs)?;
        let transaction_events = events
            .find::<v_anchor_bn254::events::Transaction>()
            .flatten()
            .collect::<Vec<_>>();
        for event in transaction_events {
            // fetch tree
            let tree_addrs = RuntimeApi::storage()
                .merkle_tree_bn254()
                .trees(event.tree_id);
            let tree = client
                .storage()
                .at(Some(at_hash))
                .await?
                .fetch(&tree_addrs)
                .await?;

            let tree = match tree {
                Some(t) => t,
                None => {
                    return Err(webb_relayer_utils::Error::Generic(
                        "No tree found",
                    ))
                }
            };
            // pallet index
            let pallet_index = {
                let metadata = client.metadata();
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
            let src_target_system =
                webb_proposals::TargetSystem::Substrate(target);
            let src_resource_id = webb_proposals::ResourceId::new(
                src_target_system,
                src_chain_id,
            );
            let mut merkle_root = [0; 32];
            merkle_root.copy_from_slice(&root.encode());

            let linked_anchors = self
                .bridge_registry_backend
                .config_or_dkg_bridges(&self.linked_anchors, &src_resource_id)
                .await?;

            // update linked anchors
            for linked_anchor in linked_anchors {
                let target_resource_id = match linked_anchor {
                    LinkedAnchorConfig::Raw(target) => {
                        let bytes: [u8; 32] = target.resource_id.into();
                        webb_proposals::ResourceId::from(bytes)
                    }
                    _ => unreachable!("unsupported"),
                };
                // Proposal proposed metric
                metrics.lock().await.anchor_update_proposals.inc();
                match target_resource_id.target_system() {
                    webb_proposals::TargetSystem::ContractAddress(_) => {
                        let proposal =
                            proposal_handler::evm_anchor_update_proposal(
                                merkle_root,
                                latest_leaf_index,
                                target_resource_id,
                                src_resource_id,
                            );
                        proposal_handler::handle_proposal(
                            &proposal,
                            &self.proposal_signing_backend,
                            metrics_clone.clone(),
                        )
                        .await?
                    }
                    webb_proposals::TargetSystem::Substrate(_) => {
                        let proposal =
                            proposal_handler::substrate_anchor_update_proposal(
                                merkle_root,
                                latest_leaf_index,
                                target_resource_id,
                                src_resource_id,
                            );
                        proposal_handler::handle_proposal(
                            &proposal,
                            &self.proposal_signing_backend,
                            metrics_clone.clone(),
                        )
                        .await?
                    }
                };
            }
            // mark this event as processed.
            let events_bytes = &event.encode();
            store.store_event(events_bytes)?;
            metrics.lock().await.total_transaction_made.inc();
        }
        Ok(())
    }
}
