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

use super::{HttpProvider, VAnchorContractWrapper};
use ethereum_types::H256;
use std::sync::Arc;
use webb::evm::contract::protocol_solidity::VAnchorContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_relayer_store::SledStore;
use webb_relayer_store::{EncryptedOutputCacheStore, EventHashStore};

/// An Encrypted Output Handler that handles `NewCommitment` events and saves the encrypted_output to the store.
/// It serves as a cache for encrypted_output that could be used by dApp for proof generation.
#[derive(Copy, Clone, Debug, Default)]
pub struct VAnchorEncryptedOutputHandler;

#[async_trait::async_trait]
impl super::EventHandler for VAnchorEncryptedOutputHandler {
    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> crate::Result<()> {
        use VAnchorContractEvents::*;
        match event {
            NewCommitmentFilter(deposit) => {
                let encrypted_output_bytes = deposit.encrypted_output.clone();
                let encrypted_output = deposit.encrypted_output.to_vec();
                let encrypted_output_index = deposit.index.as_u32();
                let value = (encrypted_output_index, encrypted_output.clone());
                let chain_id = wrapper.contract.client().get_chainid().await?;
                store.insert_encrypted_output(
                    (chain_id, wrapper.contract.address()),
                    &[value.clone()],
                )?;
                store.insert_last_deposit_block_number_for_encrypted_output(
                    (chain_id, wrapper.contract.address()),
                    log.block_number,
                )?;
                let events_bytes = serde_json::to_vec(&deposit)?;
                store.store_event(&events_bytes)?;
                tracing::trace!(
                    %log.block_number,
                    "detected block number",
                );
                tracing::event!(
                    target: crate::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %crate::probe::Kind::EncryptedOutputStore,
                    encrypted_output_index = %value.0,
                    encrypted_output = %encrypted_output_bytes,
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
                    "Encrypted Output {:?} inserted at index {} on time {}",
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
