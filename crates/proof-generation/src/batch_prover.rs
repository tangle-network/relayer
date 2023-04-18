use ark_bn254::{Bn254, Fr};
use ark_circom::{read_zkey, WitnessCalculator};
use ark_groth16::{Proof as ArkProof, ProvingKey};
use ark_relations::r1cs::ConstraintMatrices;
use circom_proving::{circom_from_folder, generate_proof, ProofError};
use num_bigint::BigInt;
use std::{fs::File, sync::Mutex};

pub struct BatchProofInput {
    pub old_root: BigInt,
    pub new_root: BigInt,
    pub path_indices: BigInt,
    pub path_elements: Vec<BigInt>,
    pub leaves: Vec<BigInt>,
    pub args_hash: BigInt,
}

pub struct MaspBatchProver {
    pub wc: &'static Mutex<WitnessCalculator>,
    pub zkey: (ProvingKey<Bn254>, ConstraintMatrices<Fr>),
}

impl MaspBatchProver {
    pub fn new(zkey_path: &str, wasm_path: &str) -> Self {
        let mut file = File::open(zkey_path).unwrap();
        let zkey = read_zkey(&mut file).unwrap();

        let wc = circom_from_folder(wasm_path);

        Self { wc, zkey }
    }

    pub fn gen_proof(
        &self,
        proof_input: BatchProofInput,
    ) -> Result<(ArkProof<Bn254>, Vec<Fr>), ProofError> {
        let inputs_for_proof = [
            ("oldRoot", vec![proof_input.old_root.clone()]),
            ("newRoot", vec![proof_input.new_root.clone()]),
            ("pathIndices", vec![proof_input.path_indices.clone()]),
            ("pathElements", proof_input.path_elements.clone()),
            ("leaves", proof_input.leaves.clone()),
            ("argsHash", vec![proof_input.args_hash]),
        ];
        let num_inputs = self.zkey.1.num_instance_variables;

        let (proof, full_assignment) =
            generate_proof(self.wc, &self.zkey, inputs_for_proof.clone())?;
        let inputs_for_verification = &full_assignment[1..num_inputs];
        Ok((proof, inputs_for_verification.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circom_proving::verify_proof;
    use num_bigint::Sign;
    use std::str::FromStr;

    #[test]
    #[ignore = "fails when this runs with the other proof test in sequence"]
    fn test_gen_proof() {
        let zkey_path =
            "../../tests/solidity-fixtures/batch-tree/4/circuit_final.zkey";
        let wasm_path = "../../tests/solidity-fixtures/batch-tree/4/batchMerkleTreeUpdate_4.wasm";
        let prover = MaspBatchProver::new(zkey_path, wasm_path);

        // pre-generated inputs
        let old_root = BigInt::from_str("19476726467694243150694636071195943429153087843379888650723427850220480216251").unwrap();
        let new_root = BigInt::from_str("20916481022825571888408619237229551581730547986478389731865872078730284403598").unwrap();
        let path_indices = BigInt::from(0);
        let path_elements = [
            "15126246733515326086631621937388047923581111613947275249184377560170833782629",
            "6404200169958188928270149728908101781856690902670925316782889389790091378414",
            "17903822129909817717122288064678017104411031693253675943446999432073303897479",
            "11423673436710698439362231088473903829893023095386581732682931796661338615804",
            "10494842461667482273766668782207799332467432901404302674544629280016211342367",
            "17400501067905286947724900644309270241576392716005448085614420258732805558809",
            "7924095784194248701091699324325620647610183513781643345297447650838438175245",
            "3170907381568164996048434627595073437765146540390351066869729445199396390350",
            "21224698076141654110749227566074000819685780865045032659353546489395159395031",
            "18113275293366123216771546175954550524914431153457717566389477633419482708807",
            "1952712013602708178570747052202251655221844679392349715649271315658568301659",
            "18071586466641072671725723167170872238457150900980957071031663421538421560166",
            "9993139859464142980356243228522899168680191731482953959604385644693217291503",
            "14825089209834329031146290681677780462512538924857394026404638992248153156554",
            "4227387664466178643628175945231814400524887119677268757709033164980107894508",
            "177945332589823419436506514313470826662740485666603469953512016396504401819",
            "4236715569920417171293504597566056255435509785944924295068274306682611080863",
            "8055374341341620501424923482910636721817757020788836089492629714380498049891"
        ].map(|str_val| BigInt::from_str(str_val).unwrap());
        let leaves = [
            "0x2c01fb4e73e2fddb08de19a3d9d6bed12b7e82fe0fb857443c28cd804c121a3b",
            "0x15ac3d30fd4b211eab59a344dc48daf12a78d8cd3c410615e978e0089644b46a",
            "0x1f42c6d6a0b3d7f5cec6be642d8b87d8457ed60dca8382a06f0c58978c1c0a94",
            "0x24f90c50ec7a935db677cc5d548f0f3cdf917a56bbd3217d2006054edeb278f6"
        ].map(|hex_val| BigInt::from_bytes_be(Sign::Plus, &hex::decode(&hex_val[2..]).unwrap()));
        let args_hash = BigInt::from_str("14154321161286109651686014554828843243662400761161800455141360660751023937653").unwrap();

        let (proof, inputs_for_verification) = prover
            .gen_proof(BatchProofInput {
                old_root,
                new_root,
                path_indices,
                path_elements: path_elements.to_vec(),
                leaves: leaves.to_vec(),
                args_hash,
            })
            .unwrap();

        let did_proof_work =
            verify_proof(&prover.zkey.0.vk, &proof, inputs_for_verification)
                .unwrap();

        assert!(did_proof_work, "failed proof verification");
    }
    #[test]
    fn test_gen_proof_fails_with_incorrect_root() {
        let zkey_path =
            "../../tests/solidity-fixtures/batch-tree/4/circuit_final.zkey";
        let wasm_path = "../../tests/solidity-fixtures/batch-tree/4/batchMerkleTreeUpdate_4.wasm";
        let prover = MaspBatchProver::new(zkey_path, wasm_path);

        // pre-generated inputs
        let old_root = BigInt::from_str("19476726467694243150694636071195943429153087843379888650723427850220480216251").unwrap();
        let new_root = BigInt::from_str("0").unwrap();
        let path_indices = BigInt::from(0);
        let path_elements = [
            "15126246733515326086631621937388047923581111613947275249184377560170833782629",
            "6404200169958188928270149728908101781856690902670925316782889389790091378414",
            "17903822129909817717122288064678017104411031693253675943446999432073303897479",
            "11423673436710698439362231088473903829893023095386581732682931796661338615804",
            "10494842461667482273766668782207799332467432901404302674544629280016211342367",
            "17400501067905286947724900644309270241576392716005448085614420258732805558809",
            "7924095784194248701091699324325620647610183513781643345297447650838438175245",
            "3170907381568164996048434627595073437765146540390351066869729445199396390350",
            "21224698076141654110749227566074000819685780865045032659353546489395159395031",
            "18113275293366123216771546175954550524914431153457717566389477633419482708807",
            "1952712013602708178570747052202251655221844679392349715649271315658568301659",
            "18071586466641072671725723167170872238457150900980957071031663421538421560166",
            "9993139859464142980356243228522899168680191731482953959604385644693217291503",
            "14825089209834329031146290681677780462512538924857394026404638992248153156554",
            "4227387664466178643628175945231814400524887119677268757709033164980107894508",
            "177945332589823419436506514313470826662740485666603469953512016396504401819",
            "4236715569920417171293504597566056255435509785944924295068274306682611080863",
            "8055374341341620501424923482910636721817757020788836089492629714380498049891"
        ].map(|str_val| BigInt::from_str(str_val).unwrap());
        let leaves = [
            "0x2c01fb4e73e2fddb08de19a3d9d6bed12b7e82fe0fb857443c28cd804c121a3b",
            "0x15ac3d30fd4b211eab59a344dc48daf12a78d8cd3c410615e978e0089644b46a",
            "0x1f42c6d6a0b3d7f5cec6be642d8b87d8457ed60dca8382a06f0c58978c1c0a94",
            "0x24f90c50ec7a935db677cc5d548f0f3cdf917a56bbd3217d2006054edeb278f6"
        ].map(|hex_val| BigInt::from_bytes_be(Sign::Plus, &hex::decode(&hex_val[2..]).unwrap()));
        let args_hash = BigInt::from_str("14154321161286109651686014554828843243662400761161800455141360660751023937653").unwrap();

        let proof = prover.gen_proof(BatchProofInput {
            old_root,
            new_root,
            path_indices,
            path_elements: path_elements.to_vec(),
            leaves: leaves.to_vec(),
            args_hash,
        });

        let (proof, inputs_for_verification) = proof.unwrap();
        let did_proof_work =
            verify_proof(&prover.zkey.0.vk, &proof, inputs_for_verification)
                .unwrap();

        assert!(!did_proof_work, "worked???????");
    }
}
