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

use serde::{Deserialize, Serialize};
// use webb::types::ElementTrait;
//     // protocol_substrate_runtime::api::runtime_types::webb_primitives::runtime::ElementTrait,
//     // subxt::{tx::PairSigner, SubstrateConfig},
// };

/// Contains data that is relayed to the Mixers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerRelayTransaction<Id, P, E, I, B> {
    /// one of the supported chains of this relayer
    pub chain_id: u64,
    /// The tree id of the mixer's underlying tree
    pub id: Id,
    /// The zero-knowledge proof bytes
    pub proof: P,
    /// The target merkle root for the proof
    pub root: E,
    /// The nullifier_hash for the proof
    pub nullifier_hash: E,
    /// The recipient of the transaction
    pub recipient: I,
    /// The relayer of the transaction
    pub relayer: I,
    /// The relayer's fee for the transaction
    pub fee: B,
    /// The refund for the transaction in native tokens
    pub refund: B,
}

/// Contains data that is relayed to the Anchors
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorRelayTransaction<Id, P, R, E, I, B> {
    /// one of the supported chains of this relayer
    pub chain_id: u64,
    /// The tree id of the mixer's underlying tree
    pub id: Id,
    /// The zero-knowledge proof bytes
    pub proof: P,
    /// The target merkle root for the proof
    pub roots: R,
    /// The nullifier_hash for the proof
    pub nullifier_hash: E,
    /// The recipient of the transaction
    pub recipient: I,
    /// The relayer of the transaction
    pub relayer: I,
    /// The relayer's fee for the transaction
    pub fee: B,
    /// The refund for the transaction in native tokens
    pub refund: B,
    /// The refresh commitment
    pub refresh_commitment: E,
    /// The external data hash,
    pub ext_data_hash: E,
}

/// Proof data object for VAnchor proofs on any chain
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofData<P, R, E> {
    /// Encoded proof
    pub proof: P,
    /// Public amount for proof
    pub public_amount: E,
    /// Root set for proving membership of inputs within
    pub roots: R,
    /// Input nullifiers to be spent
    pub input_nullifiers: Vec<E>,
    /// Output commitments to be added into the tree
    pub output_commitments: Vec<E>,
    /// External data hash consisting of arbitrary data inputs
    pub ext_data_hash: E,
    /// Root extension
    pub extension_roots: R,
}

/// External data for the VAnchor on any chain.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtData<E, I, B, A, T> {
    /// Recipient identifier of the withdrawn funds
    pub recipient: I,
    /// Relayer identifier of the transaction
    pub relayer: I,
    /// External amount being deposited or withdrawn withdrawn
    pub ext_amount: A,
    /// Fee to pay the relayer
    pub fee: B,
    /// Refund amount
    pub refund: B,
    /// Token address
    pub token: T,
    /// First encrypted output commitment
    pub encrypted_output1: E,
    /// Second encrypted output commitment
    pub encrypted_output2: E,
}

/// Contains data that is relayed to the VAnchors
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VAnchorRelayTransaction<Id, P, R, E, I, B, A, T> {
    /// one of the supported chains of this relayer
    pub chain_id: u64,
    /// The tree id of the mixer's underlying tree
    pub id: Id,
    /// The zero-knowledge proof data structure for VAnchor transactions
    pub proof_data: ProofData<P, R, E>,
    /// The external data structure for arbitrary inputs
    pub ext_data: ExtData<P, I, B, A, T>,
}

/// Proof data object for VAnchor proofs on any chain
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MASPProofData<P, R, E> {
    /// Encoded proof
    pub proof: P,
    /// Public amount for proof
    pub public_amount: E,
    /// External data hash consisting of arbitrary data inputs
    pub ext_data_hash: E,
    /// Public asset id for utxos;
    pub public_asset_id: E,
    /// Public token id for utxos;
    pub public_token_id: E,
    /// Input nullifiers to be spent
    pub input_nullifiers: Vec<E>,
    /// Output commitments to be added into the tree
    pub output_commitments: Vec<E>,
    /// Root set for proving membership of inputs within
    pub roots: R,
    /// Root extension
    pub extension_roots: R,
    /// ak_alpha x coordinate
    pub ak_alpha_x: Vec<E>,
    /// ak_alpha y coordinate
    pub ak_alpha_y: Vec<E>,
    /// whitelisted asset ids
    pub whitelisted_asset_ids: Vec<E>,
    /// fee input nullifiers to be spent
    pub fee_input_nullifiers: Vec<E>,
    /// fee output commitments to be added into the tree
    pub fee_output_commitments: Vec<E>,
    /// ak_alpha x coordinate
    pub fee_ak_alpha_x: Vec<E>,
    /// ak_alpha y coordinate
    pub fee_ak_alpha_y: Vec<E>,
}
//
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MASPRelayTransaction<Id, P, R, E, I, B, A, T> {
    /// one of the supported chains of this relayer
    pub chain_id: u64,
    /// The tree id of the mixer's underlying tree
    pub id: Id,
    /// The zero-knowledge proof data structure for VAnchor transactions
    pub proof_data: MASPProofData<P, R, E>,
    /// The external data structure for arbitrary inputs
    pub ext_data: ExtData<P, I, B, A, T>,
}
