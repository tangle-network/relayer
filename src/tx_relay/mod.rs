use serde::{Deserialize, Serialize};

pub mod evm;
pub mod substrate;

/// Contains data that is relayed to the Mixers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerRelayTransaction<Id, P, E, I, B> {
    /// one of the supported chains of this relayer
    pub chain: String,
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
    pub chain: String,
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProofData<P, R, E> {
	pub proof: P,
	pub public_amount: E,
	pub roots: R,
	pub input_nullifiers: Vec<E>,
	pub output_commitments: Vec<E>,
	pub ext_data_hash: E,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtData<E, I, B, A> {
	pub recipient: I,
	pub relayer: I,
	pub ext_amount: A,
	pub fee: B,
	pub encrypted_output1: E,
	pub encrypted_output2: E,
}

/// Contains data that is relayed to the VAnchors
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VAnchorRelayTransaction<Id, P, R, E, I, B, A> {
    /// one of the supported chains of this relayer
    pub chain: String,
    /// The tree id of the mixer's underlying tree
    pub id: Id,
    /// The zero-knowledge proof data structure for VAnchor transactions
    pub proof_data: ProofData<P, R, E>,
    /// The external data structure for arbitrary inputs
    pub ext_data: ExtData<E, I, B, A>,
}