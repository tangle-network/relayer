// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

//! In this example we will show how to use the webb relayer in depth.

#[cfg(test)]
mod tests {
    use webb::evm::contract::protocol_solidity::fungible_token_wrapper::FungibleTokenWrapperContract;
    use webb::evm::ethers::core::rand::thread_rng;
    use webb::evm::ethers::utils::parse_ether;
    use webb_relayer_handler_utils::{
        EvmCommandType, EvmVanchorCommand, WebbI256,
    };
    use webb_relayer_tx_relay_utils::{
        ExtData as RelayerExtData, ProofData as RelayerProodData,
        VAnchorRelayTransaction,
    };

    use webb::evm::ethers::types::{Bytes, U256};
    use webb::evm::{
        contract::protocol_solidity::variable_anchor_tree::VAnchorTreeContract,
        ethers::signers::{LocalWallet, Signer},
    };

    use crate::utils::{
        get_git_root_path, get_hermes_bridge_info, send_evm_tx,
        start_hermes_chain, vanchor_deposit_setup, vanchor_withdraw_setup,
    };
    use webb_relayer::service::build_web_services;
    use webb_relayer_context::RelayerContext;

    #[tokio::test]
    async fn test_tx_relaying() {
        dotenv::dotenv().unwrap();
        let git_root_path = get_git_root_path();

        // start hermes chain
        let hermes_chain = start_hermes_chain();
        let hermes_bridge = get_hermes_bridge_info();

        let secret_key = hermes_chain.keys()[0].clone();
        let deployer_wallet1 =
            LocalWallet::from(secret_key.clone()).with_chain_id(5001u32);

        // Start the relayer
        let config_path =
            git_root_path.join("relayer-tests/chain/relayer-configs");
        let config = webb_relayer_config::utils::load(config_path).unwrap();
        let store = webb_relayer_store::sled::SledStore::temporary().unwrap();
        let ctx = RelayerContext::new(config, store.clone()).await.unwrap();
        let server_handle = tokio::spawn(build_web_services(ctx.clone()));
        webb_relayer::service::ignite(ctx.clone(), std::sync::Arc::new(store))
            .await
            .unwrap();

        // Vanchor instance on hermes chain
        let vanchor = VAnchorTreeContract::new(
            hermes_bridge.vanchor,
            hermes_chain.client(),
        );
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

        let root = vanchor.get_last_root().call().await.unwrap();
        let neighbor_roots =
            vanchor.get_latest_neighbor_roots().call().await.unwrap();
        let typed_source_chain_id = hermes_chain.typed_chain_id();
        let types_target_chain_id = hermes_chain.typed_chain_id();

        let vanchor_tx_setup = vanchor_deposit_setup(
            typed_source_chain_id,
            types_target_chain_id,
            relayer_wallet.address(),
            recipient_wallet.address(),
            hermes_bridge.fungible_token_wrapper,
            root,
            neighbor_roots,
        );
        let proof_bytes = vanchor_tx_setup.proof.encode().unwrap();
        let tx = vanchor.transact(
            proof_bytes.into(),
            [0u8; 32].into(),
            vanchor_tx_setup.common_ext_data,
            vanchor_tx_setup.public_inputs.clone(),
            vanchor_tx_setup.encryptions,
        );
        tx.send()
            .await
            .map_err(|e| e.decode_revert::<String>())
            .unwrap();

        println!(" Deposit successful");

        // Withdraw tokens on hermes chain.
        let root = vanchor.get_last_root().call().await.unwrap();
        let neighbor_roots =
            vanchor.get_latest_neighbor_roots().call().await.unwrap();
        let vanchor_withdraw_setup = vanchor_withdraw_setup(
            typed_source_chain_id,
            types_target_chain_id,
            relayer_wallet.address(),
            recipient_wallet.address(),
            hermes_bridge.fungible_token_wrapper,
            root,
            neighbor_roots,
            vanchor_tx_setup.output_utxos, // output_utxos from deposit
        );
        let proof_bytes = vanchor_withdraw_setup.proof.encode().unwrap();

        // Check recipient balance before withdrawal should be 0.
        let balance_before = fungible_token_wrapper
            .balance_of(recipient_wallet.address())
            .call()
            .await
            .unwrap();

        println!("Recipient balance before: {}", balance_before);
        assert_eq!(balance_before, U256::zero());

        let vanchor_cmd = VAnchorRelayTransaction {
            proof_data: RelayerProodData {
                proof: Bytes(proof_bytes.clone().into()),
                roots: vanchor_withdraw_setup
                    .public_inputs
                    .roots
                    .clone()
                    .into(),
                extension_roots: Bytes(b"0x".to_vec().into()),
                input_nullifiers: vanchor_withdraw_setup
                    .public_inputs
                    .input_nullifiers
                    .clone(),
                output_commitments: vanchor_withdraw_setup
                    .public_inputs
                    .output_commitments
                    .to_vec(),
                public_amount: vanchor_withdraw_setup
                    .public_inputs
                    .public_amount
                    .into(),
                ext_data_hash: vanchor_withdraw_setup
                    .public_inputs
                    .ext_data_hash
                    .into(),
            },
            ext_data: RelayerExtData {
                recipient: vanchor_withdraw_setup.common_ext_data.recipient,
                relayer: vanchor_withdraw_setup.common_ext_data.relayer,
                ext_amount: WebbI256(
                    vanchor_withdraw_setup.common_ext_data.ext_amount.into(),
                ),
                fee: vanchor_withdraw_setup.common_ext_data.fee.into(),
                refund: vanchor_withdraw_setup.common_ext_data.refund.into(),
                token: vanchor_withdraw_setup.common_ext_data.token,
                encrypted_output1: vanchor_withdraw_setup
                    .encryptions
                    .encrypted_output_1,
                encrypted_output2: vanchor_withdraw_setup
                    .encryptions
                    .encrypted_output_2,
            },
        };
        let evm_cmd: EvmVanchorCommand = EvmCommandType::VAnchor(vanchor_cmd);
        let response =
            send_evm_tx(hermes_chain.chain_id(), vanchor.address(), evm_cmd)
                .await;
        match response {
            Ok(response) => {
                let status = response.status();
                assert_eq!(status, 200);
            }
            Err(_) => {
                assert!(false);
            }
        }

        // Check recipient balance after withdrawal should be 10.
        let balance = fungible_token_wrapper
            .balance_of(recipient_wallet.address())
            .call()
            .await
            .unwrap();
        assert_eq!(balance, U256::from(10));

        // Shutdown chains.
        hermes_chain.shutdown();
        // Shutdown relayer
        ctx.shutdown();
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_submit_invalid_proof() {
        dotenv::dotenv().unwrap();
        let git_root_path = get_git_root_path();

        // start hermes chain
        let hermes_chain = start_hermes_chain();
        let hermes_bridge = get_hermes_bridge_info();

        let secret_key = hermes_chain.keys()[0].clone();
        let deployer_wallet1 =
            LocalWallet::from(secret_key.clone()).with_chain_id(5001u32);

        // Start the relayer
        let config_path =
            git_root_path.join("relayer-tests/chain/relayer-configs");
        let config = webb_relayer_config::utils::load(config_path).unwrap();
        let store = webb_relayer_store::sled::SledStore::temporary().unwrap();
        let ctx = RelayerContext::new(config, store.clone()).await.unwrap();
        let server_handle = tokio::spawn(build_web_services(ctx.clone()));
        webb_relayer::service::ignite(ctx.clone(), std::sync::Arc::new(store))
            .await
            .unwrap();

        // Vanchor instance on hermes chain
        let vanchor = VAnchorTreeContract::new(
            hermes_bridge.vanchor,
            hermes_chain.client(),
        );
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

        let root = vanchor.get_last_root().call().await.unwrap();
        let neighbor_roots =
            vanchor.get_latest_neighbor_roots().call().await.unwrap();
        let typed_source_chain_id = hermes_chain.typed_chain_id();
        let types_target_chain_id = hermes_chain.typed_chain_id();

        let vanchor_tx_setup = vanchor_deposit_setup(
            typed_source_chain_id,
            types_target_chain_id,
            relayer_wallet.address(),
            recipient_wallet.address(),
            hermes_bridge.fungible_token_wrapper,
            root,
            neighbor_roots,
        );
        let proof_bytes = vanchor_tx_setup.proof.encode().unwrap();
        let tx = vanchor.transact(
            proof_bytes.into(),
            [0u8; 32].into(),
            vanchor_tx_setup.common_ext_data,
            vanchor_tx_setup.public_inputs.clone(),
            vanchor_tx_setup.encryptions,
        );
        tx.send()
            .await
            .map_err(|e| e.decode_revert::<String>())
            .unwrap();

        println!(" Deposit successful");

        // Withdraw tokens on hermes chain.
        let root = vanchor.get_last_root().call().await.unwrap();
        let neighbor_roots =
            vanchor.get_latest_neighbor_roots().call().await.unwrap();
        let vanchor_withdraw_setup = vanchor_withdraw_setup(
            typed_source_chain_id,
            types_target_chain_id,
            relayer_wallet.address(),
            recipient_wallet.address(),
            hermes_bridge.fungible_token_wrapper,
            root,
            neighbor_roots,
            vanchor_tx_setup.output_utxos, // output_utxos from deposit
        );
        let proof_bytes = vanchor_withdraw_setup.proof.encode().unwrap();
        let mut proof_input = vanchor_withdraw_setup.public_inputs.clone();
        proof_input.input_nullifiers[0] = U256::zero();
        let tx = vanchor.transact(
            proof_bytes.into(),
            [0u8; 32].into(),
            vanchor_withdraw_setup.common_ext_data,
            proof_input,
            vanchor_withdraw_setup.encryptions,
        );
        let maybe_res = tx.send().await;
        match maybe_res {
            Ok(_) => {}
            Err(e) => {
                assert_eq!(
                    "Invalid withdraw proof".to_string(),
                    e.decode_revert::<String>().unwrap()
                );
            }
        }

        // Shutdown chains.
        hermes_chain.shutdown();
        // Shutdown relayer
        ctx.shutdown();
        server_handle.abort();
    }
}
