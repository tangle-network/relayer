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

use super::VAnchorContractWrapper;
use ark_bn254::Fr as Bn254Fr;
use ark_ff::{BigInteger, PrimeField};
use arkworks_native_gadgets::merkle_tree::SparseMerkleTree;
use arkworks_native_gadgets::poseidon::Poseidon;
use arkworks_setups::common::setup_params;
use arkworks_setups::Curve;
use arkworks_utils::bytes_vec_to_f;
use ethereum_types::{H256, U256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::variable_anchor::VAnchorContractEvents;
use webb::evm::ethers::prelude::LogMeta;
use webb::evm::ethers::types;
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_store::SledStore;
use webb_relayer_store::{EventHashStore, LeafCacheStore};
use webb_relayer_types::EthersTimeLagClient;
use webb_relayer_utils::metric;
use webb_relayer_utils::Error;

/// An VAnchor Leaves Handler that handles `NewCommitment` events and saves the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.

type MerkleTree = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;

pub struct VAnchorLeavesHandler {
    mt: Arc<Mutex<MerkleTree>>,
    hasher: Poseidon<Bn254Fr>,
    chain_id: types::U256,
}

impl VAnchorLeavesHandler {
    /// Creates a new Leaves Handler for the given contract address.
    /// on the given chain id.
    ///
    /// Using the storage, it will try to load any old leaves and
    /// construct the merkle tree in memory.
    pub fn new(
        chain_id: types::U256,
        contract_address: types::Address,
        storage: Arc<SledStore>,
        empty_leaf: Vec<u8>,
    ) -> webb_relayer_utils::Result<Self> {
        let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
        let poseidon = Poseidon::<Bn254Fr>::new(params);
        let empty_leaf_scalar: Vec<Bn254Fr> = bytes_vec_to_f(&vec![empty_leaf]);
        let empty_leaf_vec = empty_leaf_scalar
            .get(0)
            .map(|d| d.into_repr().to_bytes_be())
            .ok_or(webb_relayer_utils::Error::ConvertLeafScalarError)?;

        let target_system = TargetSystem::new_contract_address(
            contract_address.to_fixed_bytes(),
        );
        let typed_chain_id = TypedChainId::Evm(chain_id.as_u32());
        let history_store_key = ResourceId::new(target_system, typed_chain_id);
        // Load all the old leaves
        let leaves = storage.get_leaves(history_store_key)?;
        let mut batch: BTreeMap<u32, Bn254Fr> = BTreeMap::new();
        for (i, leaf) in leaves.into_iter() {
            tracing::trace!(
                leaf_index = i,
                leaf = hex::encode(leaf.as_bytes()),
                "Inserting leaf into merkle tree",
            );

            let leaf: Bn254Fr =
                Bn254Fr::from_be_bytes_mod_order(leaf.as_bytes());
            batch.insert(i as _, leaf);
        }
        let mt = MerkleTree::new(&batch, &poseidon, &empty_leaf_vec)?;
        tracing::debug!(
            root = hex::encode(mt.root().into_repr().to_bytes_be()),
            "Loaded merkle tree from store",
        );

        Ok(Self {
            chain_id,
            mt: Arc::new(Mutex::new(mt)),
            hasher: poseidon,
        })
    }
}

#[async_trait::async_trait]
impl EventHandler for VAnchorLeavesHandler {
    type Contract = VAnchorContractWrapper<EthersTimeLagClient>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        (events, _log): (Self::Events, LogMeta),
        _wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use VAnchorContractEvents::*;
        let has_event = matches!(events, InsertionFilter(_));
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
        let mut batch: BTreeMap<u32, Bn254Fr> = BTreeMap::new();
        let mut mt = self.mt.lock().await;
        // We will clone the tree to compare it with the new one.
        let mt_snapshot = mt.tree.clone();

        match event {
            InsertionFilter(event_data) => {
                let commitment: [u8; 32] = event_data.commitment.into();
                let leaf_index = event_data.leaf_index;
                let value = (leaf_index, commitment.to_vec());
                let target_system = TargetSystem::new_contract_address(
                    wrapper.contract.address().to_fixed_bytes(),
                );
                let typed_chain_id = TypedChainId::Evm(self.chain_id.as_u32());
                let history_store_key =
                    ResourceId::new(target_system, typed_chain_id);

                // 1. We will validate leaf before inserting it into store
                let leaf: Bn254Fr =
                    Bn254Fr::from_be_bytes_mod_order(commitment.as_slice());
                batch.insert(leaf_index, leaf);
                mt.insert_batch(&batch, &self.hasher)?;
                // If leaf index is even number then we don't need to verify commitment
                if event_data.leaf_index % 2 == 0 {
                    tracing::debug!(
                        leaf_index = leaf_index,
                        commitment = hex::encode(commitment.as_slice()),
                        "Verified commitment",
                    );
                } else {
                    // We will verify commitment
                    let root_bytes = mt.root().into_repr().to_bytes_be();
                    let root = U256::from_big_endian(root_bytes.as_slice());
                    let valid = root.eq(&event_data.new_merkle_root);

                    tracing::debug!(
                        %valid,
                        %leaf_index,
                        ?root,
                        ?event_data.new_merkle_root,
                        "New commitment need to be verified",
                    );

                    if !valid {
                        tracing::warn!(
                            %leaf_index,
                            ?root,
                            ?event_data.new_merkle_root,
                            "Invalid merkle root. Maybe invalid leaf?"
                        );
                        // Restore previous state of the tree.
                        mt.tree = mt_snapshot;
                        return Err(Error::InvalidMerkleRootError(leaf_index));
                    }
                }
                tracing::trace!(
                    %log.block_number,
                    "detected block number",
                );
                // 2. We will insert leaf and last deposit block number into store
                store.insert_leaves_and_last_deposit_block_number(
                    history_store_key,
                    &[value.clone()],
                    log.block_number.as_u64(),
                )?;
                let events_bytes = serde_json::to_vec(&event_data)?;
                store.store_event(&events_bytes)?;
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::LeavesStore,
                    leaf_index = %value.0,
                    leaf = %hex::encode(value.1),
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
            NewCommitmentFilter(v) => {
                tracing::debug!(
                    "Leaf {:?} inserted at index {}",
                    H256::from(&v.commitment.into()),
                    v.leaf_index,
                );
            }
            _ => {
                tracing::trace!("Unhandled event {:?}", event);
            }
        };

        Ok(())
    }
}
