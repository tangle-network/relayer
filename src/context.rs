use std::convert::TryFrom;
use std::time::Duration;

use anyhow::Context;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;

use crate::config;

#[derive(Clone)]
pub struct RelayerContext {
    pub config: config::WebbRelayerConfig,
}

impl RelayerContext {
    pub fn new(config: config::WebbRelayerConfig) -> Self {
        Self { config }
    }

    pub async fn evm_provider(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<Provider<Http>> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        let provider = Provider::try_from(chain_config.http_endpoint.as_str())?
            .interval(Duration::from_millis(5u64));
        Ok(provider)
    }

    pub async fn evm_wallet(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<LocalWallet> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        let key = SecretKey::from_bytes(chain_config.private_key.as_bytes())?;
        let chain_id = chain_config.chain_id;
        let wallet = LocalWallet::from(key).with_chain_id(chain_id);
        Ok(wallet)
    }

    pub fn fee_percentage(&self, chain_name: &str) -> anyhow::Result<f64> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        Ok(chain_config.withdraw_fee_percentage)
    }
}
