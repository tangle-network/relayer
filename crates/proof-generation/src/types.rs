use serde::{Deserialize, Serialize};
/// Proof data object for Masp proof delegation. This include the private variables.
pub enum ProofGenerationError {
    ParseBigIntError,
    JsonDecodeError,
}

///
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaspDelegatedProofInputsJson {
    pub public_amount: String,
    pub ext_data_hash: String,
    pub asset_id: String,
    pub token_id: String,
    pub public_asset_id: String,
    pub public_token_id: String,

    // data for transaction inputs
    pub input_nullifier: Vec<String>,
    pub in_amount: Vec<String>,
    pub in_blinding: Vec<String>,
    pub in_path_indices: Vec<String>,
    pub in_path_elements: Vec<Vec<String>>,

    // signature data
    pub in_signature: String,
    pub in_r8x: String,
    pub in_r8y: String,

    // data for transaction outputs
    pub output_commitment: Vec<String>,
    pub out_amount: Vec<String>,
    pub out_chain_id: Vec<String>,
    pub out_pk_x: Vec<String>,
    pub out_pk_y: Vec<String>,
    pub out_blinding: Vec<String>,

    // signature data
    pub out_signature: String,
    pub out_r8x: String,
    pub out_r8y: String,

    pub chain_id: String,
    pub roots: Vec<String>,

    pub ak_x: String,
    pub ak_y: String,

    pub whitelisted_asset_ids: Vec<String>,
    pub fee_asset_id: String,
    pub fee_token_id: String,

    // data for transaction inputs
    pub fee_input_nullifier: Vec<String>,
    pub fee_in_amount: Vec<String>,
    pub fee_in_blinding: Vec<String>,
    pub fee_in_path_indices: Vec<String>,
    pub fee_in_path_elements: Vec<Vec<String>>,

    // signature data
    pub fee_in_signature: String,
    pub fee_in_r8x: String,
    pub fee_in_r8y: String,

    // data for transaction outputs
    pub fee_output_commitment: Vec<String>,
    pub fee_out_amount: Vec<String>,
    pub fee_out_chain_id: Vec<String>,
    pub fee_out_pk_x: Vec<String>,
    pub fee_out_pk_y: Vec<String>,
    pub fee_out_blinding: Vec<String>,

    // signature data
    pub fee_out_signature: String,
    pub fee_out_r8x: String,
    pub fee_out_r8y: String,

    pub fee_ak_x: String,
    pub fee_ak_y: String,
}
