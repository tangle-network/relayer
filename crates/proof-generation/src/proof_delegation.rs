use crate::types::MaspDelegatedProofInputsJson;
use ark_bn254::{Bn254, Fr};
use ark_circom::{read_zkey, WitnessCalculator};
use ark_groth16::{Proof as ArkProof, ProvingKey};
use ark_relations::r1cs::ConstraintMatrices;
use circom_proving::{
    circom_from_folder, generate_proof, ProofError, ProverPath,
};
use itertools::izip;
use num_bigint::{BigInt, ParseBigIntError};
use serde::{Deserialize, Serialize};
use std::{fs::File, str::FromStr, sync::Mutex};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspAssetInfo {
    pub asset_id: BigInt,
    pub token_id: BigInt,
}
impl MaspAssetInfo {
    pub fn from_str_values(
        asset_id: &str,
        token_id: &str,
    ) -> Result<Self, ParseBigIntError> {
        Ok(Self {
            asset_id: BigInt::from_str(asset_id)?,
            token_id: BigInt::from_str(token_id)?,
        })
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspInUtxo {
    /// The nullifier of the utxo
    pub nullifier: BigInt,
    /// The amount of the utxo
    pub amount: BigInt,
    /// The blinding of the utxo
    pub blinding: BigInt,
    /// Path indices for utxo
    pub path_indices: BigInt,
    /// Path elements for utxo
    pub path_elements: Vec<BigInt>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspSignature {
    pub signature: BigInt,
    pub r8x: BigInt,
    pub r8y: BigInt,
}
impl MaspSignature {
    pub fn from_str_values(
        signature: &str,
        r8x: &str,
        r8y: &str,
    ) -> Result<Self, ParseBigIntError> {
        Ok(Self {
            signature: BigInt::from_str(signature)?,
            r8x: BigInt::from_str(r8x)?,
            r8y: BigInt::from_str(r8y)?,
        })
    }
}

impl MaspInUtxo {
    pub fn from_str_values(
        nullifier: &str,
        amount: &str,
        blinding: &str,
        path_indices: &str,
        path_elements: Vec<String>,
    ) -> Result<Self, ParseBigIntError> {
        Ok(Self {
            nullifier: BigInt::from_str(nullifier)?,
            amount: BigInt::from_str(amount)?,
            blinding: BigInt::from_str(blinding)?,
            path_indices: BigInt::from_str(path_indices)?,
            path_elements: path_elements
                .iter()
                .map(|x| BigInt::from_str(x).expect("Error parsing BigInt"))
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MaspOutUtxo {
    /// Commitment of the utxo
    pub commitment: BigInt,
    /// chain_id of the utxo
    pub chain_id: BigInt,
    /// public key ((x, y) values) of the utxo
    pub pk: Point,
    /// amount of the utxo
    pub amount: BigInt,
    /// blinding of the utxo
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
    ) -> Result<Self, ParseBigIntError> {
        Ok(Self {
            commitment: BigInt::from_str(commitment)?,
            chain_id: BigInt::from_str(chain_id)?,
            pk: AuthKey {
                x: BigInt::from_str(pk_x)?,
                y: BigInt::from_str(pk_y)?,
            },
            amount: BigInt::from_str(amount)?,
            blinding: BigInt::from_str(blinding)?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Point {
    pub x: BigInt,
    pub y: BigInt,
}
type AuthKey = Point;
impl Point {
    pub fn from_str_values(x: &str, y: &str) -> Result<Self, ParseBigIntError> {
        Ok(Point {
            x: BigInt::from_str(x)?,
            y: BigInt::from_str(y)?,
        })
    }
}

/// Proof data object for Masp proof delegation. This include the private variables.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaspDelegatedProofInput {
    /// Public amount of transaction
    pub public_amount: BigInt,
    /// ext data hash of transaction
    pub ext_data_hash: BigInt,
    // private asset information for transaction
    pub asset: MaspAssetInfo,
    // public asset information for transaction
    pub public_asset: MaspAssetInfo,

    // data for transaction inputs
    pub in_utxos: Vec<MaspInUtxo>,

    pub in_signature: MaspSignature,
    pub auth_key: AuthKey,

    // data for transaction outputs
    pub out_utxos: Vec<MaspOutUtxo>,

    pub out_signature: MaspSignature,
    /// Chain ID of the transaction
    pub chain_id: BigInt,
    /// roots for membership check on MASP
    pub roots: Vec<BigInt>,

    pub whitelisted_asset_ids: Vec<BigInt>,
    pub fee_asset: MaspAssetInfo,

    // data for fee transaction inputs
    pub fee_in_utxos: Vec<MaspInUtxo>,

    pub fee_in_signature: MaspSignature,
    // data for fee transaction outputs
    pub fee_out_utxos: Vec<MaspOutUtxo>,
    pub fee_out_signature: MaspSignature,
    pub fee_auth_key: AuthKey,
}

impl MaspDelegatedProofInput {
    pub fn from_json(path: &str) -> Result<Self, ParseBigIntError> {
        let file =
            File::open(path).expect("Could not find file at provided path");
        let stringified_inputs: MaspDelegatedProofInputsJson =
            serde_json::from_reader(&file).expect(
                "Failed to produce MaspDelegatedProofInputsJson from file",
            );
        println!("stringified_inputs: {:#?}", stringified_inputs);

        let public_amount =
            BigInt::from_str(&stringified_inputs.public_amount)?;
        let ext_data_hash =
            BigInt::from_str(&stringified_inputs.ext_data_hash)?;

        let asset = MaspAssetInfo::from_str_values(
            &stringified_inputs.asset_id,
            &stringified_inputs.token_id,
        )?;

        let public_asset = MaspAssetInfo::from_str_values(
            &stringified_inputs.public_asset_id,
            &stringified_inputs.public_token_id,
        )?;
        // get in_utxos
        let mut in_utxos: Vec<MaspInUtxo> = vec![];
        for (nullifier, amount, blinding, path_indices, path_elements) in izip!(
            &stringified_inputs.input_nullifier,
            &stringified_inputs.in_amount,
            &stringified_inputs.in_blinding,
            &stringified_inputs.in_path_indices,
            stringified_inputs.in_path_elements,
        ) {
            let utxo = MaspInUtxo::from_str_values(
                nullifier,
                amount,
                blinding,
                path_indices,
                path_elements.clone(),
            )?;
            in_utxos.push(utxo);
        }
        // get input signature
        let in_signature = MaspSignature::from_str_values(
            &stringified_inputs.in_signature,
            &stringified_inputs.in_r8x,
            &stringified_inputs.in_r8y,
        )?;

        // get out_utxos
        let mut out_utxos: Vec<MaspOutUtxo> = vec![];
        for (commitment, chain_id, pk_x, pk_y, amount, blinding) in izip!(
            &stringified_inputs.output_commitment,
            &stringified_inputs.out_chain_id,
            &stringified_inputs.out_pk_x,
            &stringified_inputs.out_pk_y,
            &stringified_inputs.out_amount,
            &stringified_inputs.out_blinding,
        ) {
            let utxo = MaspOutUtxo::from_str_values(
                commitment, chain_id, pk_x, pk_y, amount, blinding,
            )?;
            out_utxos.push(utxo);
        }
        // get output signature
        let out_signature = MaspSignature::from_str_values(
            &stringified_inputs.out_signature,
            &stringified_inputs.out_r8x,
            &stringified_inputs.out_r8y,
        )?;

        let chain_id = BigInt::from_str(&stringified_inputs.chain_id)?;
        let roots_result: Result<Vec<BigInt>, ParseBigIntError> =
            stringified_inputs
                .roots
                .iter()
                .map(|x| BigInt::from_str(x))
                .collect();
        let roots: Vec<BigInt> = roots_result?;
        let auth_key = AuthKey::from_str_values(
            &stringified_inputs.ak_x,
            &stringified_inputs.ak_y,
        )?;

        let whitelisted_asset_ids_result: Result<
            Vec<BigInt>,
            ParseBigIntError,
        > = stringified_inputs
            .whitelisted_asset_ids
            .iter()
            .map(|x| BigInt::from_str(x))
            .collect();
        let whitelisted_asset_ids: Vec<BigInt> = whitelisted_asset_ids_result?;

        let fee_asset = MaspAssetInfo {
            asset_id: BigInt::from_str(&stringified_inputs.fee_asset_id)?,
            token_id: BigInt::from_str(&stringified_inputs.fee_token_id)?,
        };
        //
        // // get fee_in_utxos
        let mut fee_in_utxos: Vec<MaspInUtxo> = vec![];
        for (nullifier, amount, blinding, path_indices, path_elements) in izip!(
            &stringified_inputs.fee_input_nullifier,
            &stringified_inputs.fee_in_amount,
            &stringified_inputs.fee_in_blinding,
            &stringified_inputs.fee_in_path_indices,
            stringified_inputs.fee_in_path_elements,
        ) {
            let utxo = MaspInUtxo::from_str_values(
                nullifier,
                amount,
                blinding,
                path_indices,
                path_elements.clone(),
            )?;
            fee_in_utxos.push(utxo);
        }
        // get input signature
        let fee_in_signature = MaspSignature::from_str_values(
            &stringified_inputs.fee_in_signature,
            &stringified_inputs.fee_in_r8x,
            &stringified_inputs.fee_in_r8y,
        )?;

        // get fee_out_utxos
        let mut fee_out_utxos: Vec<MaspOutUtxo> = vec![];
        for (commitment, chain_id, pk_x, pk_y, amount, blinding) in izip!(
            &stringified_inputs.fee_output_commitment,
            &stringified_inputs.fee_out_chain_id,
            &stringified_inputs.fee_out_pk_x,
            &stringified_inputs.fee_out_pk_y,
            &stringified_inputs.fee_out_amount,
            &stringified_inputs.fee_out_blinding,
        ) {
            let utxo = MaspOutUtxo::from_str_values(
                commitment, chain_id, pk_x, pk_y, amount, blinding,
            )?;
            fee_out_utxos.push(utxo);
        }

        // get input signature
        let fee_out_signature = MaspSignature::from_str_values(
            &stringified_inputs.fee_out_signature,
            &stringified_inputs.fee_out_r8x,
            &stringified_inputs.fee_out_r8y,
        )?;
        let fee_auth_key = AuthKey::from_str_values(
            &stringified_inputs.fee_ak_x,
            &stringified_inputs.fee_ak_y,
        )?;
        Ok(Self {
            public_amount,
            ext_data_hash,
            asset,
            public_asset,
            in_utxos,
            in_signature,
            auth_key,
            out_utxos,
            out_signature,
            chain_id,
            roots,
            whitelisted_asset_ids,
            fee_asset,
            fee_in_utxos,
            fee_in_signature,
            fee_out_utxos,
            fee_out_signature,
            fee_auth_key,
        })
    }

    pub fn preprocess(&self) -> [(&'static str, Vec<BigInt>); 49] {
        let in_path_elements_flattened: Vec<BigInt> = self
            .in_utxos
            .iter()
            .flat_map(|utxo| utxo.path_elements.clone())
            .collect();

        let fee_in_path_elements_flattened: Vec<BigInt> = self
            .fee_in_utxos
            .iter()
            .flat_map(|utxo| utxo.path_elements.clone())
            .collect();

        return [
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
            ("inPathElements", in_path_elements_flattened),
            ("inSignature", vec![self.in_signature.signature.clone()]),
            ("inR8x", vec![self.in_signature.r8x.clone()]),
            ("inR8y", vec![self.in_signature.r8y.clone()]),
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
            ("outSignature", vec![self.out_signature.signature.clone()]),
            ("outR8x", vec![self.out_signature.r8x.clone()]),
            ("outR8y", vec![self.out_signature.r8y.clone()]),
            ("chainID", vec![self.chain_id.clone()]),
            ("roots", self.roots.clone()),
            ("ak_X", vec![self.auth_key.x.clone()]),
            ("ak_Y", vec![self.auth_key.y.clone()]),
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
            ("feeInPathElements", fee_in_path_elements_flattened),
            (
                "feeInSignature",
                vec![self.fee_in_signature.signature.clone()],
            ),
            ("feeInR8x", vec![self.fee_in_signature.r8x.clone()]),
            ("feeInR8y", vec![self.fee_in_signature.r8y.clone()]),
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
                "feeOutSignature",
                vec![self.fee_out_signature.signature.clone()],
            ),
            ("feeOutR8x", vec![self.fee_out_signature.r8x.clone()]),
            ("feeOutR8y", vec![self.fee_out_signature.r8y.clone()]),
            ("fee_ak_X", vec![self.fee_auth_key.x.clone()]),
            ("fee_ak_Y", vec![self.fee_auth_key.y.clone()]),
        ];
    }
}

#[derive(Debug, Clone)]
pub struct MaspDelegatedProver {
    pub wc: &'static Mutex<WitnessCalculator>,
    pub zkey: (ProvingKey<Bn254>, ConstraintMatrices<Fr>),
}

impl MaspDelegatedProver {
    pub fn new(path: ProverPath) -> Self {
        let mut file = File::open(path.zkey)
            .expect("Could not find file at provided path");
        let zkey = read_zkey(&mut file).expect("Failed to read zkey");

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
        let inputs_for_verification = &full_assignment
            .get(1..num_inputs)
            .expect(
            "could not slice full_assignment to get inputs_for_verification",
        );
        // todo!();
        Ok((proof, inputs_for_verification.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circom_proving::verify_proof;

    #[test]
    #[ignore]
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

        // assert!(false);
        assert!(did_proof_work, "failed proof verification");
    }
}
