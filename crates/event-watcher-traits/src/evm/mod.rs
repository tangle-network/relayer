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

#![warn(missing_docs)]
//! # EVM Events Watcher Traits 🕸️

use futures::prelude::*;
use std::cmp;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::ethers::{
    contract,
    providers::{self, Middleware},
    types,
    types::transaction,
};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_store::{
    BridgeCommand, BridgeKey, EventHashStore, HistoryStore, ProposalStore,
    QueueStore,
};
use webb_relayer_utils::metric;

/// Event watching traits
mod event_watcher;
pub use event_watcher::*;

/// Bridge watching traits
mod bridge_watcher;
pub use bridge_watcher::*;

use ark_bn254::Fr as Bn254Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_std::collections::BTreeMap;
use arkworks_native_gadgets::{
    merkle_tree::SparseMerkleTree,
    poseidon::{FieldHasher, Poseidon},
};
use arkworks_setups::{common::setup_params, Curve};
use arkworks_utils::{bytes_vec_to_f, parse_vec};

fn compute_merkle_root(relayer_leaves: Vec<[u8; 32]>) -> Vec<u8> {
    let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
    let poseidon = Poseidon::<Bn254Fr>::new(params);
    let leaves: Vec<Bn254Fr> = relayer_leaves
        .iter()
        .map(|leaf| Bn254Fr::from_be_bytes_mod_order(leaf))
        .collect();
    let pairs: BTreeMap<u32, Bn254Fr> = leaves
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, *l))
        .collect();
    // create merkle tree
    type merkle_tree = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;
    let default_leaf_hex = vec![
        "0x2fe54c60d3acabf3343a35b6eba15db4821b340f76e741e2249685ed4899af6c",
    ];
    let default_leaf_scalar: Vec<Bn254Fr> =
        bytes_vec_to_f(&parse_vec(default_leaf_hex).unwrap());
    let default_leaf_vec = default_leaf_scalar[0].into_repr().to_bytes_be();
    let smt = merkle_tree::new(&pairs, &poseidon, &default_leaf_vec).unwrap();
    let root = smt.root().into_repr().to_bytes_be();
    return root;
}
