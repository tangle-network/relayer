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

use crate::VAnchorContractWrapper;
use ethereum_types::H256;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::variable_anchor::VAnchorContractEvents;
use webb::evm::ethers::prelude::LogMeta;
use webb::evm::ethers::types;
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposal_signing_backends::proposal_handler;
use webb_proposal_signing_backends::queue::policy::ProposalPolicy;
use webb_proposal_signing_backends::queue::{
    ProposalsQueue, QueuedAnchorUpdateProposal,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_store::SledStore;
use webb_relayer_store::{EventHashStore, HistoryStore};
use webb_relayer_types::EthersTimeLagClient;
use webb_relayer_utils::metric;

/// Represents an VAnchor Contract Watcher which will use a configured signing backend for signing proposals.
#[derive(typed_builder::TypedBuilder)]
pub struct VAnchorDepositHandler<Q, P> {
    #[builder(setter(into))]
    chain_id: types::U256,
    #[builder(setter(into))]
    store: Arc<SledStore>,
    proposals_queue: Q,
    policy: P,
}

#[async_trait::async_trait]
impl<Q, P> EventHandler for VAnchorDepositHandler<Q, P>
where
    Q: ProposalsQueue<Proposal = QueuedAnchorUpdateProposal> + Send + Sync,
    P: ProposalPolicy + Send + Sync + Clone,
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

        let linked_anchors = match &wrapper.config.linked_anchors {
            Some(anchors) => anchors,
            None => {
                tracing::error!(
                    "Linked anchors not configured for : ({})",
                    self.chain_id
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
                _ => {
                    unreachable!("Only evm chains are supported for now.")
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
