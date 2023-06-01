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

use super::VAnchorContractWrapper;
use ethereum_types::H256;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::VAnchorContractEvents;
use webb::evm::ethers::prelude::LogMeta;
use webb::evm::ethers::types;
use webb_event_watcher_traits::evm::EventHandler;
use webb_event_watcher_traits::EthersTimeLagClient;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_store::SledStore;
use webb_relayer_store::{EncryptedOutputCacheStore, EventHashStore};
use webb_relayer_utils::metric;

/// An Encrypted Output Handler that handles `NewCommitment` events and saves the encrypted_output to the store.
/// It serves as a cache for encrypted_output that could be used by dApp for proof generation.
#[derive(Copy, Clone, Debug)]
pub struct VAnchorEncryptedOutputHandler {
    chain_id: types::U256,
}

impl VAnchorEncryptedOutputHandler {
    pub fn new(chain_id: types::U256) -> Self {
        Self { chain_id }
    }
}

#[async_trait::async_trait]
impl EventHandler for VAnchorEncryptedOutputHandler {
    type Contract = VAnchorContractWrapper<EthersTimeLagClient>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        (events, _meta): (Self::Events, LogMeta),
        _wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use VAnchorContractEvents::*;
        let has_event = matches!(events, NewCommitmentFilter(_));
        Ok(has_event)
    }

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
                let encrypted_output_bytes = deposit.encrypted_output.clone();
                let encrypted_output = deposit.encrypted_output.to_vec();
                let encrypted_output_index = deposit.leaf_index.as_u32();
                let value = (encrypted_output_index, encrypted_output);
                let target_system = TargetSystem::new_contract_address(
                    wrapper.contract.address().to_fixed_bytes(),
                );
                let typed_chain_id = TypedChainId::Evm(self.chain_id.as_u32());
                let history_store_key =
                    ResourceId::new(target_system, typed_chain_id);

                store.insert_encrypted_output_and_last_deposit_block_number(
                    history_store_key,
                    &[value.clone()],
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
                    kind = %webb_relayer_utils::probe::Kind::EncryptedOutputStore,
                    encrypted_output_index = %value.0,
                    encrypted_output = %hex::encode(encrypted_output_bytes),
                    chain_id = %self.chain_id,
                    block_number = %log.block_number
                );
            }
            EdgeAdditionFilter(v) => {
                let merkle_root: [u8; 32] = v.merkle_root.into();
                tracing::debug!(
                    "Edge Added of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(merkle_root)
                );
            }
            EdgeUpdateFilter(v) => {
                let merkle_root: [u8; 32] = v.merkle_root.into();
                tracing::debug!(
                    "Edge Updated of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(merkle_root)
                );
            }
            NewNullifierFilter(v) => {
                tracing::debug!(
                    "new nullifier {} found",
                    H256::from(&v.nullifier.into())
                );
            }
            InsertionFilter(v) => {
                tracing::debug!(
                    "Encrypted Output {:?} inserted at index {} on time {}",
                    H256::from(&v.commitment.into()),
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
