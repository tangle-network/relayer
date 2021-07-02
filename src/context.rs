use std::convert::TryFrom;
use std::time::Duration;

use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;

use crate::chains::evm::{ChainName, EvmChain};
use crate::config;

#[derive(Clone)]
pub struct RelayerContext {
    config: config::WebbRelayerConfig,
}

impl RelayerContext {
    pub fn new(config: config::WebbRelayerConfig) -> Self { Self { config } }

    pub async fn evm_provider<C: EvmChain>(
        &self,
    ) -> anyhow::Result<Provider<Http>> {
        let endpoint = C::endpoint();
        let provider =
            Provider::try_from(endpoint)?.interval(Duration::from_millis(5u64));
        Ok(provider)
    }

    pub async fn evm_wallet<C: EvmChain>(&self) -> anyhow::Result<LocalWallet> {
        let evm = &self.config.evm;
        match C::name() {
            ChainName::Edgeware => todo!(),
            ChainName::Webb => todo!(),
            ChainName::Ganache if evm.ganache.is_some() => {
                let c = evm.ganache.clone().unwrap();
                let pk = c.private_key;
                let key = SecretKey::from_bytes(pk.as_bytes())?;
                let wallet = LocalWallet::from(key);
                Ok(wallet)
            },
            ChainName::Beresheet => todo!(),
            ChainName::Harmoney => todo!(),
            _ => todo!(),
        }
    }
}
