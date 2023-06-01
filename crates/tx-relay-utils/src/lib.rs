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

/// Contains data that is relayed to the Anchors
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorRelayTransaction<Id, P, R, E, I, B> {
    /// one of the supported chains of this relayer
    pub chain_id: u64,
    /// The tree id of the vanchor's underlying tree
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
    /// The tree id of the vanchor's underlying tree
    pub id: Id,
    /// The zero-knowledge proof data structure for VAnchor transactions
    pub proof_data: ProofData<P, R, E>,
    /// The external data structure for arbitrary inputs
    pub ext_data: ExtData<P, I, B, A, T>,
}
