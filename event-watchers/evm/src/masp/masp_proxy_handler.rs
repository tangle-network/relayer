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

use super::{HttpProvider, MaspBatchProver, MaspProxyContractWrapper};
use ark_ff::{BigInteger, PrimeField};
use arkworks_native_gadgets::poseidon::Poseidon;
use arkworks_setups::common::setup_params;
use arkworks_setups::Curve;
use arkworks_utils::bytes_vec_to_f;
use circom_proving::ProverPath;
use ethereum_types::{H256, U256};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::MaspProxyContractEvents;
use webb::evm::ethers::prelude::{LogMeta, Middleware};
use webb_event_watcher_traits::evm::EventHandler;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_store::{EventHashStore, LeafCacheStore};
use webb_relayer_store::{HistoryStore, SledStore};
use webb_relayer_utils::metric;

use ark_bn254::Fr as Bn254Fr;
use arkworks_native_gadgets::merkle_tree::SparseMerkleTree;

/// An Masp Proxy Handler that handles queued deposit events and saves the leaves to the store. It
/// generates a proof from queued deposits once the queue (on the contract) reaches `batch_size`
/// elements and submits it on-chain

type MerkleTree = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;

type Bytes = Vec<u8>;

pub struct MaspProxyHandler {
    mt: Arc<Mutex<MerkleTree>>,
    storage: Arc<SledStore>,
    prover: MaspBatchProver,
    // queue: Arc<Mutex<Vec<Bytes>>>,
    // current_queue_size: usize,
    batch_size: usize,
}

impl MaspProxyHandler {
    pub fn new(
        storage: Arc<SledStore>,
        default_leaf: Vec<u8>,
        prover_path: ProverPath,
    ) -> Self {
        let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
        let poseidon = Poseidon::<Bn254Fr>::new(params);
        let default_leaf_scalar: Vec<Bn254Fr> =
            bytes_vec_to_f(&vec![default_leaf]);
        let default_leaf_vec = default_leaf_scalar[0].into_repr().to_bytes_be();
        let pairs: BTreeMap<u32, Bn254Fr> = BTreeMap::new();
        let mt = MerkleTree::new(&pairs, &poseidon, &default_leaf_vec).unwrap();
        let batch_size = 4;
        let current_queue_size = 0;

        let prover = MaspBatchProver::new(&prover_path.zkey, &prover_path.wasm);

        Self {
            mt: Arc::new(Mutex::new(mt)),
            storage,
            prover,
            // queue: Arc::new(Mutex::new(vec![])),
            batch_size,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for MaspProxyHandler {
    type Contract = MaspProxyContractWrapper<HttpProvider>;

    type Events = MaspProxyContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: Self::Events,
        wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use MaspProxyContractEvents::*;
        // In this we will validate queued deposits we are trying to save is valid with following steps
        // 1. Check if contract queue size is of at least batch_size elements
        // 2. We create a local merkle tree and check if current root matches contract merkle root
        // 3. We insert all leaves into local merkle tree
        // 4. Compute new root.
        // 5. Compute proof for batch insertion using `MaspBatchProver`
        // 6. Submit proof to contract
        match events {
            QueueDepositFilter(event_data) => {
                // let latest_index = wrapper.contract.next_queue_erc20_deposit_index
                // let queue = wrapper
                //     .contract
                //     .queue_erc20_deposit_map(event_data.proxied_masp)
                //     .await;
                // let (proof, inputs_for_verification) = self
                //     .prover
                //     .gen_proof(BatchProofInput {
                //         old_root,
                //         new_root,
                //         path_indices,
                //         path_elements: path_elements.to_vec(),
                //         leaves: leaves.to_vec(),
                //         args_hash,
                //     })
                //     .unwrap();
                // let did_proof_work = verify_proof(
                //     &prover.zkey.0.vk,
                //     &proof,
                //     inputs_for_verification,
                // )
                // .unwrap();
                //
                // assert!(did_proof_work, "failed proof verification");
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
        use MaspProxyContractEvents::*;
        match event {
            // NewCommitmentFilter(deposit) => {
            //     let commitment: [u8; 32] = deposit.commitment.into();
            //     let leaf_index = deposit.leaf_index.as_u32();
            //     let value = (leaf_index, commitment.to_vec());
            //     let chain_id = wrapper.contract.client().get_chainid().await?;
            //     let target_system = TargetSystem::new_contract_address(
            //         wrapper.contract.address().to_fixed_bytes(),
            //     );
            //     let typed_chain_id = TypedChainId::Evm(chain_id.as_u32());
            //     let history_store_key =
            //         ResourceId::new(target_system, typed_chain_id);
            //     store.insert_leaves(history_store_key, &[value.clone()])?;
            //     store.insert_last_deposit_block_number(
            //         history_store_key,
            //         log.block_number.as_u64(),
            //     )?;
            //     let events_bytes = serde_json::to_vec(&deposit)?;
            //     store.store_event(&events_bytes)?;
            //     tracing::trace!(
            //         %log.block_number,
            //         "detected block number",
            //     );
            //     tracing::event!(
            //         target: webb_relayer_utils::probe::TARGET,
            //         tracing::Level::DEBUG,
            //         kind = %webb_relayer_utils::probe::Kind::LeavesStore,
            //         leaf_index = %value.0,
            //         leaf = %format!("{:?}", value.1),
            //         chain_id = %chain_id,
            //         block_number = %log.block_number
            //     );
            // }
            _ => {
                tracing::trace!("Unhandled event {:?}", event);
            }
        };

        Ok(())
    }
}
