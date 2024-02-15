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

//! In this example we will show how to use the webb relayer in depth.

#![deny(unsafe_code)]
#![allow(unused_variables)]

use serde_json::json;
use std::path::PathBuf;
use std::str::FromStr;
use webb::evm::contract::protocol_solidity::fungible_token_wrapper::FungibleTokenWrapperContract;
use webb::evm::contract::protocol_solidity::variable_anchor_tree::{
    CommonExtData, Encryptions, PublicInputs,
};
use webb::evm::ethers::core::rand::thread_rng;
use webb::evm::ethers::utils::{hex, keccak256, parse_ether};
use webb_evm_test_utils::LocalEvmChain;

use webb::evm::ethers::types::{H160, U256};
use webb::evm::{
    contract::protocol_solidity::variable_anchor_tree::VAnchorTreeContract,
    ethers::signers::{LocalWallet, Signer},
};

use ark_ff::fields::PrimeField;
use ark_ff::BigInteger;
use webb_evm_test_utils::circom_proving::types::Proof as SolidityProof;
use webb_evm_test_utils::types::{ExtData, IntoAbiToken, ProofData};
use webb_evm_test_utils::utils::{
    self, deconstruct_public_inputs_el, setup_utxos, setup_vanchor_circuit,
    vanchor_2_2_fixtures,
};
use webb_evm_test_utils::v_bridge::VAnchorBridgeInfo;
use webb_relayer::service;
use webb_relayer::service::build_web_services;
use webb_relayer_config::cli::{setup_logger, Opts};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::EvmCommandType;

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

#[tokio::test]
async fn test_vanchor_deposit_and_withdraw() {
    // Get fixtures
    let git_root_path = get_git_root_path();
    let fixture_path = git_root_path.join("relayer-tests/solidity-fixtures");
    let (params_2_2, wc_2_2) = vanchor_2_2_fixtures(&fixture_path);

    // Get saved chain state with deployed contracts for testing.
    let source = git_root_path
        .join("relayer-tests/testing-chains/hermes/chain-state/state.json");
    let tmp_dir = tempfile::TempDir::with_prefix("hermes").unwrap();
    let hermes_chain_state = tmp_dir.path();
    utils::copy_saved_state(&source, hermes_chain_state);

    // Deploy Hermes chain.
    let hermes_chain = LocalEvmChain::new_with_chain_state(
        5001,
        String::from("Hermes"),
        hermes_chain_state,
        Some(8545u16),
    );

    println!("hermes endpoint: {}", hermes_chain.endpoint());
    println!(" hermes ws endpoint: {}", hermes_chain.ws_endpoint());

    let secret_key = hermes_chain.keys()[0].clone();
    let deployer_wallet1 =
        LocalWallet::from(secret_key.clone()).with_chain_id(5001u32);

    println!("Waller : {:?}", deployer_wallet1.address().to_string());
    println!("secret_key : {:?} ", hex::encode(secret_key.to_bytes()));

    let hermes_bridge = VAnchorBridgeInfo {
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
    };

    println!("Hermes bridge deployed: {:?}", hermes_bridge);

    // start relayer
    setup_logger(3i32, "webb_relayer").unwrap();
    dotenv::dotenv().unwrap();
    let config_path =
        git_root_path.join("relayer-tests/testing-chains/relayer-configs");
    let config = webb_relayer_config::utils::load(config_path).unwrap();
    let store = webb_relayer_store::sled::SledStore::temporary().unwrap();

    // finally, after loading the config files, we can build the relayer context.
    let ctx = RelayerContext::new(config, store.clone()).await.unwrap();

    // Start the web server:
    let server_handle = tokio::spawn(build_web_services(ctx.clone()));
    // and also the background services:
    // this does not block, will fire the services on background tasks.
    service::ignite(ctx.clone(), std::sync::Arc::new(store))
        .await
        .unwrap();

    // Vanchor instance on hermes chain
    let vanchor =
        VAnchorTreeContract::new(hermes_bridge.vanchor, hermes_chain.client());

    let fungible_token_wrapper = FungibleTokenWrapperContract::new(
        hermes_bridge.fungible_token_wrapper,
        hermes_chain.client(),
    );

    // Approve token spending on vanchor.
    fungible_token_wrapper
        .approve(vanchor.address(), parse_ether(1000).unwrap())
        .send()
        .await
        .unwrap();

    // Mint tokens on wallet.
    fungible_token_wrapper
        .mint(deployer_wallet1.address(), parse_ether(1000).unwrap())
        .send()
        .await
        .unwrap();

    let recipient_wallet =
        LocalWallet::new(&mut thread_rng()).with_chain_id(5002u32);
    let relayer_wallet =
        LocalWallet::new(&mut thread_rng()).with_chain_id(5001u32);

    let recipient = recipient_wallet.address();
    let relayer = relayer_wallet.address();
    let typed_source_chain_id = hermes_chain.typed_chain_id();
    let types_target_chain_id = hermes_chain.typed_chain_id();
    let ext_amount = 10_i128;
    let public_amount = 10_i128;
    let fee = 0_i128;
    let refund = 0_i128.into();
    let token = hermes_bridge.fungible_token_wrapper;

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
    let root = vanchor.get_last_root().call().await.unwrap();
    let neighbor_roots =
        vanchor.get_latest_neighbor_roots().call().await.unwrap();

    let (proof, public_inputs) = setup_vanchor_circuit(
        public_amount,
        typed_source_chain_id,
        ext_data_hash.to_vec(),
        input_utxos,
        output_utxos.clone(),
        root,
        [neighbor_roots[0]],
        leaves,
        &params_2_2,
        wc_2_2,
    );

    let solidity_proof = SolidityProof::try_from(proof).unwrap();
    let proof_bytes = solidity_proof.encode().unwrap();

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

    let tx = vanchor.transact(
        proof_bytes.into(),
        [0u8; 32].into(),
        common_ext_data,
        public_inputs.clone(),
        encryptions,
    );

    tx.send()
        .await
        .map_err(|e| e.decode_revert::<String>())
        .unwrap();

    println!(" Deposit successful");

    // Withdraw tokens on hermes chain.

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
    let root = vanchor.get_last_root().call().await.unwrap();
    let neighbor_roots =
        vanchor.get_latest_neighbor_roots().call().await.unwrap();

    let (proof, public_inputs) = setup_vanchor_circuit(
        public_amount,
        typed_source_chain_id,
        ext_data_hash_bytes.to_vec(),
        input_utxos,
        output_utxos.clone(),
        root,
        [neighbor_roots[0]],
        leaves,
        &params_2_2,
        wc_2_2,
    );

    let solidity_proof = SolidityProof::try_from(proof).unwrap();
    let proof_bytes = solidity_proof.encode().unwrap();

    let common_ext_data = CommonExtData {
        recipient: ext_data.recipient,
        ext_amount: ext_data.ext_amount.into(),
        relayer: ext_data.relayer,
        fee: ext_data.fee.into(),
        refund: ext_data.refund.into(),
        token: ext_data.token,
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
        roots: flattened_root.clone().into(),
        extension_roots: b"0x".to_vec().into(),
        input_nullifiers: nullifiers.clone(),
        output_commitments: commitments,
        public_amount,
        ext_data_hash,
    };

    let proof_data = ProofData {
        proof: proof_bytes.into(),
        roots: flattened_root.clone().into(),
        extension_roots: b"0x".to_vec().into(),
        input_nullifiers: nullifiers.clone(),
        output_commitments: commitments,
        public_amount,
        ext_data_hash,
    };

    let encryptions = Encryptions {
        encrypted_output_1: encrypted_output1.into(),
        encrypted_output_2: encrypted_output2.into(),
    };
    // Check recipient balance before withdrawal should be 0.
    let balance_before = fungible_token_wrapper
        .balance_of(recipient)
        .call()
        .await
        .unwrap();

    println!("Recipient balance before: {}", balance_before);
    assert_eq!(balance_before, U256::zero());

    let api = "http://0.0.0.0:9955/api/v1/send/evm/5001/0x68b1d87f95878fe05b998f19b66f4baba5de1aed";
    let payload = json!({
        "vAnchor": {
            "extData": {
                "recipient": hex::encode(ext_data.recipient.to_fixed_bytes()),
                "relayer": hex::encode(ext_data.relayer.to_fixed_bytes()),
                "extAmount": ext_data.ext_amount.to_string(),
                "fee": ext_data.fee,
                "refund": ext_data.refund,
                "token": hex::encode(ext_data.token.to_fixed_bytes()),
                "encryptedOutput1": hex::encode(ext_data.encrypted_output1),
                "encryptedOutput2": hex::encode(ext_data.encrypted_output2),
            },
            "proofData": {
                "proof": hex::encode(proof_data.proof),
                "roots": hex::encode(proof_data.roots),
                "extensionRoots": hex::encode(proof_data.extension_roots),
                "inputNullifiers": proof_data.input_nullifiers,
                "outputCommitments": proof_data.output_commitments,
                "publicAmount": proof_data.public_amount,
                "extDataHash": proof_data.ext_data_hash,
            },
        }
    });

    println!("Payload: {:?}", payload);
    // send post api
    let response = reqwest::Client::new().post(api).json(&payload).send().await;
    match response {
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap();
            println!("Status: {}", status);
            println!("Body: {}", body);
            assert_eq!(status, 200);
        }
        Err(e) => {
            println!("Error: {:?}", e);
            assert!(false);
        }
    }

    // Check recipient balance after withdrawal should be 10.
    let balance = fungible_token_wrapper
        .balance_of(recipient)
        .call()
        .await
        .unwrap();
    assert_eq!(balance, U256::from(10));

    // Shutdown chains.
    hermes_chain.shutdown();
}
