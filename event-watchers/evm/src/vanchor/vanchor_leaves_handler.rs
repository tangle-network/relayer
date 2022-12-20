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

use super::{HttpProvider, VAnchorContractWrapper};
use ethereum_types::H256;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::VAnchorContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_store::SledStore;
use webb_relayer_store::{EventHashStore, LeafCacheStore};
use webb_relayer_utils::metric;

/// An VAnchor Leaves Handler that handles `NewCommitment` events and saves the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.
#[derive(Copy, Clone, Debug, Default)]
pub struct VAnchorLeavesHandler;

#[async_trait::async_trait]
impl EventHandler for VAnchorLeavesHandler {
    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        use VAnchorContractEvents::*;
        match event {
            NewCommitmentFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.index.as_u32();
                let value = (leaf_index, commitment.to_vec());
                let chain_id = wrapper.contract.client().get_chainid().await?;
                let target_system = TargetSystem::new_contract_address(
                    wrapper.contract.address().to_fixed_bytes(),
                );
                let typed_chain_id = TypedChainId::Evm(chain_id.as_u32());
                let history_store_key =
                    ResourceId::new(target_system, typed_chain_id);
                store.insert_leaves(history_store_key, &[value.clone()])?;
                store.insert_last_deposit_block_number(
                    history_store_key,
                    log.block_number.as_u64(),
                )?;
                let events_bytes = serde_json::to_vec(&deposit)?;
                store.store_event(&events_bytes)?;
                tracing::trace!(
                    %log.block_number,
                    "detected block number",
                );
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::LeavesStore,
                    leaf_index = %value.0,
                    leaf = %format!("{:?}", value.1),
                    chain_id = %chain_id,
                    block_number = %log.block_number
                );
            }
            EdgeAdditionFilter(v) => {
                tracing::debug!(
                    "Edge Added of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(v.merkle_root)
                );
            }
            EdgeUpdateFilter(v) => {
                tracing::debug!(
                    "Edge Updated of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(v.merkle_root)
                );
            }
            NewNullifierFilter(v) => {
                tracing::debug!(
                    "new nullifier {} found",
                    H256::from_slice(&v.nullifier)
                );
            }
            InsertionFilter(v) => {
                tracing::debug!(
                    "Leaf {:?} inserted at index {} on time {}",
                    H256::from_slice(&v.commitment),
                    v.leaf_index,
                    v.timestamp
                );
            }
            _ => {
                tracing::trace!("Unhandled event {:?}", event);
            }
        };

        Ok(())
    }
}
