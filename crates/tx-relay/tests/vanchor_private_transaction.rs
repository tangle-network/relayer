use anvil::{spawn, NodeConfig};
use ethers::abi::Token;
use ethers::core::rand::thread_rng;
use ethers::prelude::abigen;
use ethers::prelude::SignerMiddleware;
use ethers::signers::LocalWallet;
use ethers::signers::Signer;
use ethers::utils::parse_ether;
use std::sync::Arc;
use tracing::log::LevelFilter;

abigen!(TokenWrapper, "assets/FungibleTokenWrapperContract.json");

#[tokio::test]
async fn test_vanchor() {
    let _ = env_logger::builder()
        .filter_level(LevelFilter::Info)
        .try_init();

    let governor_wallet = LocalWallet::new(&mut thread_rng());
    let relayer_wallet = LocalWallet::new(&mut thread_rng());
    let node_config = NodeConfig {
        chain_id: Some(1),
        genesis_accounts: vec![governor_wallet.clone(), relayer_wallet.clone()],
        genesis_balance: parse_ether(100000).unwrap(),
        enable_tracing: false,
        gas_limit: u64::MAX.into(),
        gas_price: Some(0.into()),
        disable_block_gas_limit: true,
        port: 0,
        ..Default::default()
    };
    let chain_one = spawn(node_config.clone()).await;
    //let chain_two = spawn(node_config).await;

    let provider = chain_one.1.http_provider();
    let client =
        Arc::new(SignerMiddleware::new(provider, governor_wallet.clone()));

    let params = Token::Tuple(vec![
        Token::String("WETH".to_string()),
        Token::String("WETH".to_string()),
    ]);
    let token_one = TokenWrapper::deploy(Arc::clone(&client), params)
        .unwrap()
        .send()
        .await
        .unwrap();

    dbg!(token_one.name().call().await.unwrap());

    let address = dbg!(governor_wallet.address());
    dbg!(token_one
        .mint(address, parse_ether(10).unwrap())
        .call()
        .await
        .unwrap());

    dbg!(token_one.balance_of(address).call().await.unwrap());

    // TODO: deploy vbridge with tokens

    /*
    let config = WebbRelayerConfig {
        port: chain_one.1.socket_address().port(),
        evm: Default::default(),
        substrate: Default::default(),
        experimental: Default::default(),
        features: Default::default(),
    };
    let store = create_store(&Opts::default()).await.unwrap();
    let ctx = RelayerContext::new(config, store);

    let command = EvmVanchorCommand{
        chain_id: chain_one.0.chain_id(),
        id: Address::default(),
        proof_data: ProofData::default(),
        ext_data: ExtData::default(),
    };
    let (my_tx, my_rx) = mpsc::channel(50);

    handle_vanchor_relay_tx(ctx, command, my_tx).await.unwrap();
    */
}
