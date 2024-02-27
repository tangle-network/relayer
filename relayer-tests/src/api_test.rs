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

#[cfg(test)]
mod tests {
    use webb::evm::contract::protocol_solidity::fungible_token_wrapper::FungibleTokenWrapperContract;
    use webb::evm::ethers::core::rand::thread_rng;
    use webb::evm::ethers::utils::parse_ether;
    use webb::evm::{
        contract::protocol_solidity::variable_anchor_tree::VAnchorTreeContract,
        ethers::signers::{LocalWallet, Signer},
    };
    use webb_relayer_config::cli::setup_logger;

    use crate::utils::{
        get_evm_leaves, get_git_root_path, get_hermes_bridge_info,
        start_hermes_chain, vanchor_deposit_setup,
    };
    use webb_relayer::service::build_web_services;
    use webb_relayer_context::RelayerContext;

    #[tokio::test]
    async fn test_data_caching_apis() {
        dotenv::dotenv().unwrap();
        let git_root_path = get_git_root_path();

        // start hermes chain
        let hermes_chain = start_hermes_chain();
        let hermes_bridge = get_hermes_bridge_info();

        let secret_key = hermes_chain.keys()[0].clone();
        let deployer_wallet1 =
            LocalWallet::from(secret_key.clone()).with_chain_id(5001u32);

        // Start the relayer
        setup_logger(3i32, "webb_relayer").unwrap();
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

        let cached_leaves =
            get_evm_leaves(hermes_chain.chain_id(), hermes_bridge.vanchor)
                .await
                .unwrap();
        println!("Statues: {:?}", cached_leaves.status());
        println!("Cached leaves: {:?}", cached_leaves.text().await.unwrap());
        assert_eq!(1, 2);

        // Shutdown chains.
        hermes_chain.shutdown();
        // Shutdown relayer
        ctx.shutdown();
        server_handle.abort();
    }
}
