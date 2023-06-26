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

use crate::VAnchorContractWrapper;
use ethereum_types::H256;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::variable_anchor::VAnchorContractEvents;
use webb::evm::ethers::prelude::LogMeta;
use webb::evm::ethers::types;
use webb_bridge_registry_backends::BridgeRegistryBackend;
use webb_event_watcher_traits::evm::EventHandler;
use webb_event_watcher_traits::EthersTimeLagClient;
use webb_proposal_signing_backends::proposal_handler;
use webb_proposal_signing_backends::queue::policy::ProposalPolicy;
use webb_proposal_signing_backends::queue::{
    ProposalsQueue, QueuedAnchorUpdateProposal,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_store::SledStore;
use webb_relayer_store::{EventHashStore, HistoryStore};
use webb_relayer_utils::metric;

/// Represents an VAnchor Contract Watcher which will use a configured signing backend for signing proposals.
#[derive(typed_builder::TypedBuilder)]
pub struct VAnchorDepositHandler<Q, P, C> {
    #[builder(setter(into))]
    chain_id: types::U256,
    #[builder(setter(into))]
    store: Arc<SledStore>,
    proposals_queue: Q,
    policy: P,
    bridge_registry_backend: C,
}

#[async_trait::async_trait]
impl<Q, P, C> EventHandler for VAnchorDepositHandler<Q, P, C>
where
    Q: ProposalsQueue<Proposal = QueuedAnchorUpdateProposal> + Send + Sync,
    P: ProposalPolicy + Send + Sync + Clone,
    C: BridgeRegistryBackend + Send + Sync,
{
    type Contract = VAnchorContractWrapper<EthersTimeLagClient>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        (events, meta): (Self::Events, LogMeta),
        wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use VAnchorContractEvents::*;
        let has_event = matches!(events, NewCommitmentFilter(event_data) if event_data.leaf_index.as_u32() % 2 != 0);
        if !has_event {
            return Ok(false);
        }
        // only handle events if we fully synced.
        let src_chain_id =
            webb_proposals::TypedChainId::Evm(self.chain_id.as_u32());
        let src_target_system =
            webb_proposals::TargetSystem::new_contract_address(
                wrapper.contract.address().to_fixed_bytes(),
            );
        let history_key =
            webb_proposals::ResourceId::new(src_target_system, src_chain_id);
        let target_block_number = self
            .store
            .get_target_block_number(history_key, meta.block_number.as_u64())?;
        let event_block = meta.block_number.as_u64();
        let allowed_margin = 10u64;
        // Check if the event is in the latest block or within the allowed margin.
        let fully_synced = event_block >= target_block_number
            || event_block.saturating_add(allowed_margin)
                >= target_block_number;
        Ok(fully_synced)
    }

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        use VAnchorContractEvents::*;
        let event_data = match event {
            NewCommitmentFilter(data) => {
                let commitment: [u8; 32] = data.commitment.into();
                let info = (data.leaf_index.as_u32(), H256::from(commitment));
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::MerkleTreeInsertion,
                    leaf_index = %info.0,
                    leaf = %info.1,
                    chain_id = %self.chain_id,
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

        let root: [u8; 32] =
            wrapper.contract.get_last_root().call().await?.into();
        let leaf_index = event_data.leaf_index.as_u32();
        let src_chain_id =
            webb_proposals::TypedChainId::Evm(self.chain_id.as_u32());
        let src_target_system =
            webb_proposals::TargetSystem::new_contract_address(
                wrapper.contract.address().to_fixed_bytes(),
            );
        let src_resource_id =
            webb_proposals::ResourceId::new(src_target_system, src_chain_id);

        let linked_anchors = self
            .bridge_registry_backend
            .config_or_dkg_bridges(
                &wrapper.config.linked_anchors,
                &src_resource_id,
            )
            .await?;
        for linked_anchor in linked_anchors {
            let target_resource_id = match linked_anchor {
                LinkedAnchorConfig::Raw(target) => {
                    let bytes: [u8; 32] = target.resource_id.into();
                    webb_proposals::ResourceId::from(bytes)
                }
                _ => unreachable!("unsupported"),
            };
            // Anchor update proposal proposed metric
            metrics.lock().await.anchor_update_proposals.inc();

            let proposal = match target_resource_id.target_system() {
                webb_proposals::TargetSystem::ContractAddress(_) => {
                    let p = proposal_handler::evm_anchor_update_proposal(
                        root,
                        leaf_index,
                        target_resource_id,
                        src_resource_id,
                    );
                    QueuedAnchorUpdateProposal::new(p)
                }
                webb_proposals::TargetSystem::Substrate(_) => {
                    let p = proposal_handler::substrate_anchor_update_proposal(
                        root,
                        leaf_index,
                        target_resource_id,
                        src_resource_id,
                    );
                    QueuedAnchorUpdateProposal::new(p)
                }
            };

            self.proposals_queue
                .enqueue(proposal, self.policy.clone())?;
        }
        // mark this event as processed.
        let events_bytes = serde_json::to_vec(&event_data)?;
        store.store_event(&events_bytes)?;
        metrics.lock().await.total_transaction_made.inc();
        Ok(())
    }
}
