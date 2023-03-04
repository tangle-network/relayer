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
use ark_ff::{BigInteger, PrimeField};
use arkworks_native_gadgets::poseidon::Poseidon;
use arkworks_setups::common::setup_params;
use arkworks_setups::Curve;
use arkworks_utils::bytes_vec_to_f;
use ethereum_types::{H256, U256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::VAnchorContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_store::{EventHashStore, LeafCacheStore};
use webb_relayer_store::{HistoryStore, SledStore};
use webb_relayer_utils::metric;

use ark_bn254::Fr as Bn254Fr;
use arkworks_native_gadgets::merkle_tree::SparseMerkleTree;

/// An VAnchor Leaves Handler that handles `NewCommitment` events and saves the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.

type MerkleTree = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;

pub struct VAnchorLeavesHandler {
    mt: Arc<Mutex<MerkleTree>>,
    storage: Arc<SledStore>,
}
impl VAnchorLeavesHandler {
    pub fn new(storage: Arc<SledStore>, default_leaf: Vec<u8>) -> Self {
        let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
        let poseidon = Poseidon::<Bn254Fr>::new(params);
        let default_leaf_scalar: Vec<Bn254Fr> =
            bytes_vec_to_f(&vec![default_leaf]);
        let default_leaf_vec = default_leaf_scalar[0].into_repr().to_bytes_be();
        let pairs: BTreeMap<u32, Bn254Fr> = BTreeMap::new();
        let mt = MerkleTree::new(&pairs, &poseidon, &default_leaf_vec).unwrap();

        Self {
            mt: Arc::new(Mutex::new(mt)),
            storage,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for VAnchorLeavesHandler {
    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: Self::Events,
        wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use VAnchorContractEvents::*;
        // In this we will validate leaf/commitment we are trying to save is valid with following steps
        // 1. We create a local merkle tree and insert leaf to it.
        // 2. Compute merkle root for tree
        // 3. Check if computed root is known root on contract
        // 4. If it fails to validate we restart syncing events
        match events {
            NewCommitmentFilter(data) => {
                let commitment: [u8; 32] = data.commitment.into();
                let leaf: Bn254Fr =
                    Bn254Fr::from_be_bytes_mod_order(commitment.as_slice());
                let mut batch: BTreeMap<u32, Bn254Fr> = BTreeMap::new();
                let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
                let poseidon = Poseidon::<Bn254Fr>::new(params);
                batch.insert(data.leaf_index.as_u32(), leaf);
                let mut mt = self.mt.lock().await;
                if mt.insert_batch(&batch, &poseidon).is_err() {
                    return Ok(true);
                }
                let root_bytes = mt.root().into_repr().to_bytes_be();
                let root: U256 = U256::from_big_endian(root_bytes.as_slice());
                let is_known_root =
                    wrapper.contract.is_known_root(root).call().await?;
                if !is_known_root {
                    // In case of invalid merkle root, relayer should clear its storage and restart syncing.
                    // 1. We define history store key which will be used to access storage.
                    let chain_id =
                        wrapper.contract.client().get_chainid().await?;
                    let target_system = TargetSystem::new_contract_address(
                        wrapper.contract.address().to_fixed_bytes(),
                    );
                    let typed_chain_id = TypedChainId::Evm(chain_id.as_u32());
                    let history_store_key =
                        ResourceId::new(target_system, typed_chain_id);

                    // 2. Clear merkle tree on relayer.
                    mt.tree.clear();
                    // 3. Clear LeafStore cache on relayer.
                    self.storage.clear_leaves_cache(history_store_key)?;
                    // 4. Reset last block no processed by leaf handler.
                    self.storage.insert_last_deposit_block_number(
                        history_store_key,
                        0,
                    )?;

                    // 5. Reset last saved block number by event watcher.
                    //    Relayer will restart syncing from block number contract was deployed at.
                    self.storage.set_last_block_number(
                        history_store_key,
                        wrapper.config.common.deployed_at,
                    )?;
                    return Ok(false);
                }
                return Ok(true);
            }
            _ => return Ok(false),
        };
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
                let commitment: [u8; 32] = deposit.commitment.into();
                let leaf_index = deposit.leaf_index.as_u32();
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
                    "Leaf {:?} inserted at index {} on time {}",
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
