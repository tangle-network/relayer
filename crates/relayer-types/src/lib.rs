use std::sync::Arc;
use webb::evm::ethers::{prelude::TimeLag, providers};
use webb_relayer_utils::multi_provider::MultiProvider;

pub mod etherscan_api;
pub mod mnemonic;
pub mod private_key;
pub mod rpc_client;
pub mod rpc_url;
pub mod suri;

/// Ethereum TimeLag client using Ethers, that includes a retry strategy.
pub type EthersTimeLagClient = TimeLag<
    Arc<
        providers::Provider<
            providers::RetryClient<MultiProvider<providers::Http>>,
        >,
    >,
>;

pub type EthersClient =
    providers::Provider<providers::RetryClient<MultiProvider<providers::Http>>>;
