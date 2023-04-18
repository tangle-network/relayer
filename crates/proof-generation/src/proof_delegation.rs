use crate::types::MaspDelegatedProofInputsJson;
use ark_bn254::{Bn254, Fr};
use ark_circom::{read_zkey, WitnessCalculator};
use ark_groth16::{Proof as ArkProof, ProvingKey};
use ark_relations::r1cs::ConstraintMatrices;
use circom_proving::{
    circom_from_folder, generate_proof, ProofError, ProverPath,
};
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::{fs::File, str::FromStr, sync::Mutex};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspAssetInfo {
    pub asset_id: BigInt,
    pub token_id: BigInt,
}
impl MaspAssetInfo {
    pub fn from_str_values(asset_id: &str, token_id: &str) -> Self {
        Self {
            asset_id: BigInt::from_str(asset_id).unwrap(),
            token_id: BigInt::from_str(token_id).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspInUtxo {
    pub nullifier: BigInt,
    pub amount: BigInt,
    pub blinding: BigInt,
    pub path_indices: BigInt,
    pub path_elements: Vec<BigInt>,
}

impl MaspInUtxo {
    pub fn from_str_values(
        nullifier: &str,
        amount: &str,
        blinding: &str,
        path_indices: &str,
        path_elements: Vec<String>,
    ) -> Self {
        Self {
            nullifier: BigInt::from_str(nullifier).unwrap(),
            amount: BigInt::from_str(&amount).unwrap(),
            blinding: BigInt::from_str(&blinding).unwrap(),
            path_indices: BigInt::from_str(&path_indices).unwrap(),
            path_elements: path_elements
                .iter()
                .map(|x| BigInt::from_str(x).unwrap())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspOutUtxo {
    pub commitment: BigInt,
    pub chain_id: BigInt,
    pub pk: Point,
    pub amount: BigInt,
    pub blinding: BigInt,
}
impl MaspOutUtxo {
    pub fn from_str_values(
        commitment: &str,
        chain_id: &str,
        pk_x: &str,
        pk_y: &str,
        amount: &str,
        blinding: &str,
    ) -> Self {
        Self {
            commitment: BigInt::from_str(commitment).unwrap(),
            chain_id: BigInt::from_str(&chain_id).unwrap(),
            pk: Point {
                x: BigInt::from_str(&pk_x).unwrap(),
                y: BigInt::from_str(&pk_y).unwrap(),
            },
            amount: BigInt::from_str(&amount).unwrap(),
            blinding: BigInt::from_str(&blinding).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspKey {
    pub ak: Point,
    pub alpha: BigInt,
    pub ak_alpha: Point,
}

impl MaspKey {
    pub fn from_str_values(
        ak_x: &str,
        ak_y: &str,
        alpha: &str,
        ak_alpha_x: &str,
        ak_alpha_y: &str,
    ) -> Self {
        Self {
            ak: Point {
                x: BigInt::from_str(&ak_x).unwrap(),
                y: BigInt::from_str(&ak_y).unwrap(),
            },
            alpha: BigInt::from_str(&alpha).unwrap(),
            ak_alpha: Point {
                x: BigInt::from_str(&ak_alpha_x).unwrap(),
                y: BigInt::from_str(&ak_alpha_y).unwrap(),
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Point {
    pub x: BigInt,
    pub y: BigInt,
}

/// Proof data object for Masp proof delegation. This include the private variables.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaspDelegatedProofInput {
    pub public_amount: BigInt,
    pub ext_data_hash: BigInt,
    pub asset: MaspAssetInfo,
    pub public_asset: MaspAssetInfo,

    // data for transaction inputs
    pub in_utxos: Vec<MaspInUtxo>,

    // data for transaction outputs
    pub out_utxos: Vec<MaspOutUtxo>,
    pub chain_id: BigInt,
    pub roots: Vec<BigInt>,

    pub keypairs: Vec<MaspKey>,
    pub whitelisted_asset_ids: Vec<BigInt>,
    pub fee_asset: MaspAssetInfo,

    pub fee_in_utxos: Vec<MaspInUtxo>,
    pub fee_out_utxos: Vec<MaspOutUtxo>,
    pub fee_keypairs: Vec<MaspKey>,
}

use serde_json::Error;
impl MaspDelegatedProofInput {
    pub fn from_json(path: &str) -> Result<Self, Error> {
        let file = File::open(path).unwrap();
        let stringified_inputs: MaspDelegatedProofInputsJson =
            serde_json::from_reader(&file).unwrap();
        // println!("raw: {:?}", stringified_inputs);
        let public_amount =
            BigInt::from_str(&stringified_inputs.public_amount).unwrap();
        let ext_data_hash =
            BigInt::from_str(&stringified_inputs.ext_data_hash).unwrap();

        let asset = MaspAssetInfo::from_str_values(
            &stringified_inputs.asset_id,
            &stringified_inputs.token_id,
        );
        let public_asset = MaspAssetInfo::from_str_values(
            &stringified_inputs.public_asset_id,
            &stringified_inputs.public_token_id,
        );
        // get in_utxos
        let mut in_utxos: Vec<MaspInUtxo> = vec![];
        for i in 0..stringified_inputs.input_nullifier.len() {
            let utxo = MaspInUtxo::from_str_values(
                &stringified_inputs.input_nullifier[i],
                &stringified_inputs.in_amount[i],
                &stringified_inputs.in_blinding[i],
                &stringified_inputs.in_path_indices[i],
                stringified_inputs.in_path_elements[i].clone(),
            );
            in_utxos.push(utxo);
        }

        // get out_utxos
        let mut out_utxos: Vec<MaspOutUtxo> = vec![];
        for i in 0..stringified_inputs.output_commitment.len() {
            let utxo = MaspOutUtxo::from_str_values(
                &stringified_inputs.output_commitment[i],
                &stringified_inputs.out_chain_id[i],
                &stringified_inputs.out_pk_x[i],
                &stringified_inputs.out_pk_y[i],
                &stringified_inputs.out_amount[i],
                &stringified_inputs.out_blinding[i],
            );
            out_utxos.push(utxo);
        }

        let chain_id = BigInt::from_str(&stringified_inputs.chain_id).unwrap();
        let roots: Vec<BigInt> = stringified_inputs
            .roots
            .iter()
            .map(|x| BigInt::from_str(x).unwrap())
            .collect();

        // get keypairs
        let mut keypairs: Vec<MaspKey> = vec![];
        for i in 0..stringified_inputs.ak_x.len() {
            let key = MaspKey::from_str_values(
                &stringified_inputs.ak_x[i],
                &stringified_inputs.ak_y[i],
                &stringified_inputs.alpha[i],
                &stringified_inputs.ak_alpha_x[i],
                &stringified_inputs.ak_alpha_y[i],
            );
            keypairs.push(key);
        }
        let whitelisted_asset_ids: Vec<BigInt> = stringified_inputs
            .whitelisted_asset_ids
            .iter()
            .map(|x| BigInt::from_str(&x).unwrap())
            .collect();
        let fee_asset = MaspAssetInfo {
            asset_id: BigInt::from_str(&stringified_inputs.fee_asset_id)
                .unwrap(),
            token_id: BigInt::from_str(&stringified_inputs.fee_token_id)
                .unwrap(),
        };
        // get fee_in_utxos
        let mut fee_in_utxos: Vec<MaspInUtxo> = vec![];
        for i in 0..stringified_inputs.fee_input_nullifier.len() {
            let utxo = MaspInUtxo::from_str_values(
                &stringified_inputs.fee_input_nullifier[i],
                &stringified_inputs.fee_in_amount[i],
                &stringified_inputs.fee_in_blinding[i],
                &stringified_inputs.fee_in_path_indices[i],
                stringified_inputs.fee_in_path_elements[i].clone(),
            );
            fee_in_utxos.push(utxo);
        }
        // get fee_out_utxos
        let mut fee_out_utxos: Vec<MaspOutUtxo> = vec![];
        for i in 0..stringified_inputs.fee_output_commitment.len() {
            let utxo = MaspOutUtxo::from_str_values(
                &stringified_inputs.fee_output_commitment[i],
                &stringified_inputs.fee_out_chain_id[i],
                &stringified_inputs.fee_out_pk_x[i],
                &stringified_inputs.fee_out_pk_y[i],
                &stringified_inputs.fee_out_amount[i],
                &stringified_inputs.fee_out_blinding[i],
            );
            fee_out_utxos.push(utxo);
        }
        // get fee_keypairs
        let mut fee_keypairs: Vec<MaspKey> = vec![];
        for i in 0..stringified_inputs.fee_ak_x.len() {
            let key = MaspKey::from_str_values(
                &stringified_inputs.fee_ak_x[i],
                &stringified_inputs.fee_ak_y[i],
                &stringified_inputs.fee_alpha[i],
                &stringified_inputs.fee_ak_alpha_x[i],
                &stringified_inputs.fee_ak_alpha_y[i],
            );
            fee_keypairs.push(key);
        }
        Ok(Self {
            public_amount,
            ext_data_hash,
            asset,
            public_asset,
            in_utxos,
            out_utxos,
            chain_id,
            roots,
            keypairs,
            whitelisted_asset_ids,
            fee_asset,
            fee_in_utxos,
            fee_out_utxos,
            fee_keypairs,
        })
    }

    pub fn preprocess(&self) -> [(&'static str, Vec<BigInt>); 43] {
        let in_path_elements_flattened: Vec<BigInt> = self
            .in_utxos
            .iter()
            .map(|utxo| utxo.path_elements.clone())
            .flatten()
            .collect();
        let fee_in_path_elements_flattened: Vec<BigInt> = self
            .fee_in_utxos
            .iter()
            .map(|utxo| utxo.path_elements.clone())
            .flatten()
            .collect();

        [
            ("publicAmount", vec![self.public_amount.clone()]),
            ("extDataHash", vec![self.ext_data_hash.clone()]),
            ("assetID", vec![self.asset.asset_id.clone()]),
            ("tokenID", vec![self.asset.token_id.clone()]),
            ("publicAssetID", vec![self.public_asset.asset_id.clone()]),
            ("publicTokenID", vec![self.public_asset.token_id.clone()]),
            (
                "inputNullifier",
                self.in_utxos
                    .iter()
                    .map(|utxo| utxo.nullifier.clone())
                    .collect(),
            ),
            (
                "inAmount",
                self.in_utxos
                    .iter()
                    .map(|utxo| utxo.amount.clone())
                    .collect(),
            ),
            (
                "inBlinding",
                self.in_utxos
                    .iter()
                    .map(|utxo| utxo.blinding.clone())
                    .collect(),
            ),
            (
                "inPathIndices",
                self.in_utxos
                    .iter()
                    .map(|utxo| utxo.path_indices.clone())
                    .collect(),
            ),
            ("inPathElements", in_path_elements_flattened.clone()),
            (
                "outputCommitment",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.commitment.clone())
                    .collect(),
            ),
            (
                "outAmount",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.amount.clone())
                    .collect(),
            ),
            (
                "outChainID",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.chain_id.clone())
                    .collect(),
            ),
            (
                "outPk_X",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.pk.x.clone())
                    .collect(),
            ),
            (
                "outPk_Y",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.pk.y.clone())
                    .collect(),
            ),
            (
                "outBlinding",
                self.out_utxos
                    .iter()
                    .map(|utxo| utxo.blinding.clone())
                    .collect(),
            ),
            ("chainID", vec![self.chain_id.clone()]),
            ("roots", self.roots.clone()),
            (
                "ak_X",
                self.keypairs.iter().map(|key| key.ak.x.clone()).collect(),
            ),
            (
                "ak_Y",
                self.keypairs.iter().map(|key| key.ak.y.clone()).collect(),
            ),
            (
                "alpha",
                self.keypairs.iter().map(|key| key.alpha.clone()).collect(),
            ),
            (
                "ak_alpha_X",
                self.keypairs
                    .iter()
                    .map(|key| key.ak_alpha.x.clone())
                    .collect(),
            ),
            (
                "ak_alpha_Y",
                self.keypairs
                    .iter()
                    .map(|key| key.ak_alpha.y.clone())
                    .collect(),
            ),
            ("feeAssetID", vec![self.fee_asset.asset_id.clone()]),
            ("whitelistedAssetIDs", self.whitelisted_asset_ids.clone()),
            ("feeTokenID", vec![self.fee_asset.token_id.clone()]),
            (
                "feeInputNullifier",
                self.fee_in_utxos
                    .iter()
                    .map(|utxo| utxo.nullifier.clone())
                    .collect(),
            ),
            (
                "feeInAmount",
                self.fee_in_utxos
                    .iter()
                    .map(|utxo| utxo.amount.clone())
                    .collect(),
            ),
            (
                "feeInBlinding",
                self.fee_in_utxos
                    .iter()
                    .map(|utxo| utxo.blinding.clone())
                    .collect(),
            ),
            (
                "feeInPathIndices",
                self.fee_in_utxos
                    .iter()
                    .map(|utxo| utxo.path_indices.clone())
                    .collect(),
            ),
            ("feeInPathElements", fee_in_path_elements_flattened.clone()),
            (
                "feeOutputCommitment",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.commitment.clone())
                    .collect(),
            ),
            (
                "feeOutAmount",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.amount.clone())
                    .collect(),
            ),
            (
                "feeOutChainID",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.chain_id.clone())
                    .collect(),
            ),
            (
                "feeOutPk_X",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.pk.x.clone())
                    .collect(),
            ),
            (
                "feeOutPk_Y",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.pk.y.clone())
                    .collect(),
            ),
            (
                "feeOutBlinding",
                self.fee_out_utxos
                    .iter()
                    .map(|utxo| utxo.blinding.clone())
                    .collect(),
            ),
            (
                "fee_ak_X",
                self.fee_keypairs
                    .iter()
                    .map(|key| key.ak.x.clone())
                    .collect(),
            ),
            (
                "fee_ak_Y",
                self.fee_keypairs
                    .iter()
                    .map(|key| key.ak.y.clone())
                    .collect(),
            ),
            (
                "fee_alpha",
                self.fee_keypairs
                    .iter()
                    .map(|key| key.alpha.clone())
                    .collect(),
            ),
            (
                "fee_ak_alpha_X",
                self.fee_keypairs
                    .iter()
                    .map(|key| key.ak_alpha.x.clone())
                    .collect(),
            ),
            (
                "fee_ak_alpha_Y",
                self.fee_keypairs
                    .iter()
                    .map(|key| key.ak_alpha.y.clone())
                    .collect(),
            ),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct MaspDelegatedProver {
    pub wc: &'static Mutex<WitnessCalculator>,
    pub zkey: (ProvingKey<Bn254>, ConstraintMatrices<Fr>),
}

impl MaspDelegatedProver {
    pub fn new(path: ProverPath) -> Self {
        let mut file = File::open(path.zkey).unwrap();
        let zkey = read_zkey(&mut file).unwrap();

        let wc = circom_from_folder(&path.wasm);

        Self { wc, zkey }
    }

    pub fn gen_proof(
        &self,
        proof_inputs: &MaspDelegatedProofInput,
    ) -> Result<(ArkProof<Bn254>, Vec<Fr>), ProofError> {
        let inputs = proof_inputs.preprocess();

        let num_inputs = self.zkey.1.num_instance_variables;

        let (proof, full_assignment) =
            generate_proof(self.wc, &self.zkey, inputs.clone())?;
        let inputs_for_verification = &full_assignment[1..num_inputs];
        // todo!();
        Ok((proof, inputs_for_verification.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // #[ignore]
    fn test_proof_delegation() {
        let zkey_path =
            "../../tests/solidity-fixtures/masp_vanchor_2/2/circuit_final.zkey"
                .to_string();
        let wasm_path = "../../tests/solidity-fixtures/masp_vanchor_2/2/masp_vanchor_2_2.wasm".to_string();

        let path = ProverPath::new(zkey_path, wasm_path);
        let prover = MaspDelegatedProver::new(path);

        let proof_input =
            MaspDelegatedProofInput::from_json("./test_data/proofInputs.json")
                .unwrap();
        let (proof, inputs_for_verification) =
            prover.gen_proof(&proof_input).unwrap();
        let did_proof_work =
            verify_proof(&prover.zkey.0.vk, &proof, inputs_for_verification)
                .unwrap();

        assert!(did_proof_work, "failed proof verification");
    }
}
