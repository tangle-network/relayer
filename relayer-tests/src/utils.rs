use ark_ff::{BigInteger, PrimeField};
use arkworks_setups::utxo::Utxo;
use reqwest::Response;
use std::{path::PathBuf, str::FromStr};
use webb::evm::contract::protocol_solidity::variable_anchor_tree::{
    CommonExtData, Encryptions, PublicInputs,
};
use webb::evm::ethers::{
    types::{H160, U256},
    utils::keccak256,
};
use webb_evm_test_utils::circom_proving::types::Proof as SolidityProof;
use webb_evm_test_utils::types::IntoAbiToken;
use webb_evm_test_utils::{
    types::ExtData,
    utils::{
        deconstruct_public_inputs_el, setup_utxos, setup_vanchor_circuit,
        vanchor_2_2_fixtures,
    },
    v_bridge::VAnchorBridgeInfo,
    LocalEvmChain,
};
use webb_relayer_handler_utils::EvmVanchorCommand;

type Bn254Fr = ark_bn254::Fr;

pub fn start_hermes_chain() -> LocalEvmChain {
    // Get saved chain state with deployed contracts for testing.
    let git_root_path = get_git_root_path();
    let source =
        git_root_path.join("relayer-tests/chain/hermes/chain-state/state.json");
    let tmp_dir = tempfile::TempDir::with_prefix("hermes").unwrap();
    let hermes_chain_state = tmp_dir.path();
    std::fs::copy(source, hermes_chain_state.join("state.json")).unwrap();

    LocalEvmChain::new_with_chain_state(
        5001,
        String::from("Hermes"),
        hermes_chain_state,
        Some(8545u16),
    )
}

pub fn get_git_root_path() -> PathBuf {
    let git_root = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("Failed to get git root")
        .stdout;
    let git_root = std::str::from_utf8(&git_root)
        .expect("Failed to parse git root")
        .trim()
        .to_string();
    PathBuf::from(&git_root)
}

// Returns bridge info for saved chain state after deploying contracts.
pub fn get_hermes_bridge_info() -> VAnchorBridgeInfo {
    VAnchorBridgeInfo {
        bridge: H160::from_str("0x5fbdb2315678afecb367f032d93f642f64180aa3")
            .unwrap(),
        vanchor: H160::from_str("0x68b1d87f95878fe05b998f19b66f4baba5de1aed")
            .unwrap(),
        vanchor_handler: H160::from_str(
            "0xe7f1725e7734ce288f8367e1bb143e90bb3f0512",
        )
        .unwrap(),
        treasury: H160::from_str("0xcf7ed3acca5a467e9e704c703e8d87f634fb0fc9")
            .unwrap(),
        treasury_handler: H160::from_str(
            "0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0",
        )
        .unwrap(),
        fungible_token_wrapper: H160::from_str(
            "0x0b306bf915c4d645ff596e518faf3f9669b97016",
        )
        .unwrap(),
        token_wrapper_handler: H160::from_str(
            "0x9a676e781a523b5d0c0e43731313a708cb607508",
        )
        .unwrap(),
    }
}

pub async fn send_evm_tx(
    chain_id: u32,
    vanchor_address: H160,
    evm_cmd: EvmVanchorCommand,
) -> Result<Response, String> {
    let api = format!(
        "http://0.0.0.0:9955/api/v1/send/evm/{}/{}",
        chain_id, vanchor_address
    );
    let payload =
        serde_json::to_string(&evm_cmd).expect("Failed to serialize JSON");
    // send post api
    let response = reqwest::Client::new()
        .post(api)
        .json(&payload)
        .send()
        .await
        .unwrap();
    Ok(response)
}

pub async fn get_evm_leaves(
    chain_id: u32,
    vanchor_address: H160,
) -> Result<Response, String> {
    let api = format!(
        "http://0.0.0.0:9955/api/v1/leaves/evm/{}/0x{}",
        chain_id,
        hex::encode(vanchor_address.0)
    );

    println!("api: {}", api);
    let response = reqwest::Client::new().get(api).send().await.unwrap();
    Ok(response)
}

pub struct VanchorTxSetup {
    pub input_utxos: [Utxo<Bn254Fr>; 2],
    pub output_utxos: [Utxo<Bn254Fr>; 2],
    pub common_ext_data: CommonExtData,
    pub public_inputs: PublicInputs,
    pub encryptions: Encryptions,
    pub proof: SolidityProof,
}
pub fn vanchor_deposit_setup(
    typed_source_chain_id: u64,
    types_target_chain_id: u64,
    relayer: H160,
    recipient: H160,
    token: H160,
    root: U256,
    neighbor_roots: Vec<U256>,
) -> VanchorTxSetup {
    let git_root_path = get_git_root_path();
    let fixture_path = git_root_path.join("relayer-tests/solidity-fixtures");
    let (params_2_2, wc_2_2) = vanchor_2_2_fixtures(&fixture_path);

    let ext_amount = 10_i128;
    let public_amount = 10_i128;
    let fee = 0_i128;
    let refund = 0_i128.into();

    let input_chain_ids = [typed_source_chain_id, types_target_chain_id];
    let input_amounts = [0, 0];
    let input_indices = [0, 1];
    let output_chain_ids = [typed_source_chain_id, types_target_chain_id];
    let output_amount = [10, 0];
    let output_indices = [0, 0];

    let input_utxos =
        setup_utxos(input_chain_ids, input_amounts, Some(input_indices));
    let output_utxos =
        setup_utxos(output_chain_ids, output_amount, Some(output_indices));

    let encrypted_output1 =
        output_utxos[0].commitment.into_repr().to_bytes_be();
    let encrypted_output2 =
        output_utxos[1].commitment.into_repr().to_bytes_be();

    let leaf0 = input_utxos[0].commitment.into_repr().to_bytes_be();
    let leaf1 = input_utxos[1].commitment.into_repr().to_bytes_be();

    let leaves: Vec<Vec<u8>> = vec![leaf0, leaf1];

    let ext_data = ExtData::builder()
        .recipient(recipient)
        .relayer(relayer)
        .ext_amount(ext_amount)
        .fee(fee.into())
        .refund(refund)
        .token(token)
        .encrypted_output1(encrypted_output1.clone())
        .encrypted_output2(encrypted_output2.clone())
        .build();

    let ext_data_hash = keccak256(ext_data.encode_abi_token());

    let (proof, public_inputs) = setup_vanchor_circuit(
        public_amount,
        typed_source_chain_id,
        ext_data_hash.to_vec(),
        input_utxos.clone(),
        output_utxos.clone(),
        root,
        [neighbor_roots[0]],
        leaves,
        &params_2_2,
        wc_2_2,
    );

    let solidity_proof = SolidityProof::try_from(proof).unwrap();

    let common_ext_data = CommonExtData {
        recipient,
        ext_amount: ext_data.ext_amount.into(),
        relayer,
        fee: ext_data.fee.into(),
        refund,
        token,
    };

    // Deconstructing public inputs
    let (
        _chain_id,
        public_amount,
        root_set,
        nullifiers,
        commitments,
        ext_data_hash,
    ) = deconstruct_public_inputs_el(&public_inputs);

    let flattened_root: Vec<u8> = root_set
        .iter()
        .flat_map(|x| {
            let mut be_bytes = [0u8; 32];
            x.to_big_endian(&mut be_bytes);
            be_bytes
        })
        .collect();
    let public_inputs = PublicInputs {
        roots: flattened_root.into(),
        extension_roots: b"0x".to_vec().into(),
        input_nullifiers: nullifiers,
        output_commitments: commitments,
        public_amount,
        ext_data_hash,
    };

    let encryptions = Encryptions {
        encrypted_output_1: encrypted_output1.into(),
        encrypted_output_2: encrypted_output2.into(),
    };

    VanchorTxSetup {
        input_utxos,
        output_utxos,
        common_ext_data,
        public_inputs,
        encryptions,
        proof: solidity_proof,
    }
}

pub fn vanchor_withdraw_setup(
    typed_source_chain_id: u64,
    types_target_chain_id: u64,
    relayer: H160,
    recipient: H160,
    token: H160,
    root: U256,
    neighbor_roots: Vec<U256>,
    output_utxos: [Utxo<Bn254Fr>; 2],
) -> VanchorTxSetup {
    let git_root_path = get_git_root_path();
    let fixture_path = git_root_path.join("relayer-tests/solidity-fixtures");
    let (params_2_2, wc_2_2) = vanchor_2_2_fixtures(&fixture_path);

    let ext_amount = -10_i128;
    let public_amount = -10_i128;
    let fee = 0_i128;
    let refund = 0_i128;

    let output_chain_ids = [typed_source_chain_id, types_target_chain_id];
    let output_amount = [0, 0];
    let output_indices = [0, 0];
    let input_utxos = output_utxos; // Use the output utxos from the previous transaction as input utxos.

    let output_utxos =
        setup_utxos(output_chain_ids, output_amount, Some(output_indices));

    let encrypted_output1 =
        output_utxos[0].commitment.into_repr().to_bytes_be();
    let encrypted_output2 =
        output_utxos[1].commitment.into_repr().to_bytes_be();

    let leaf0 = input_utxos[0].commitment.into_repr().to_bytes_be();
    let leaf1 = input_utxos[1].commitment.into_repr().to_bytes_be();

    let leaves: Vec<Vec<u8>> = vec![leaf0, leaf1];

    let ext_data = ExtData::builder()
        .recipient(recipient)
        .relayer(relayer)
        .ext_amount(ext_amount)
        .fee(fee.into())
        .refund(refund.into())
        .token(token)
        .encrypted_output1(encrypted_output1.clone())
        .encrypted_output2(encrypted_output2.clone())
        .build();

    let ext_data_hash_bytes = keccak256(ext_data.encode_abi_token());

    let (proof, public_inputs) = setup_vanchor_circuit(
        public_amount,
        typed_source_chain_id,
        ext_data_hash_bytes.to_vec(),
        input_utxos.clone(),
        output_utxos.clone(),
        root,
        [neighbor_roots[0]],
        leaves,
        &params_2_2,
        wc_2_2,
    );

    let solidity_proof = SolidityProof::try_from(proof).unwrap();

    // Deconstructing public inputs
    let (
        _chain_id,
        public_amount,
        root_set,
        nullifiers,
        commitments,
        ext_data_hash,
    ) = deconstruct_public_inputs_el(&public_inputs);

    let flattened_root: Vec<u8> = root_set
        .iter()
        .flat_map(|x| {
            let mut be_bytes = [0u8; 32];
            x.to_big_endian(&mut be_bytes);
            be_bytes
        })
        .collect();

    let common_ext_data = CommonExtData {
        recipient,
        ext_amount: ext_data.ext_amount.into(),
        relayer,
        fee: ext_data.fee.into(),
        refund: ext_data.refund.into(),
        token,
    };

    let public_inputs = PublicInputs {
        roots: flattened_root.into(),
        extension_roots: b"0x".to_vec().into(),
        input_nullifiers: nullifiers,
        output_commitments: commitments,
        public_amount,
        ext_data_hash,
    };

    let encryptions = Encryptions {
        encrypted_output_1: encrypted_output1.into(),
        encrypted_output_2: encrypted_output2.into(),
    };

    VanchorTxSetup {
        input_utxos,
        output_utxos,
        common_ext_data,
        public_inputs,
        encryptions,
        proof: solidity_proof,
    }
}
