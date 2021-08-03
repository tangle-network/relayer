use std::convert::TryFrom;
use std::time::Duration;

use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;

use crate::chains::evm::{ChainName, EvmChain};
use crate::config;

#[derive(Clone)]
pub struct RelayerContext {
    pub config: config::WebbRelayerConfig,
}

impl RelayerContext {
    pub fn new(config: config::WebbRelayerConfig) -> Self {
        Self { config }
    }

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
        // DRY
        macro_rules! wallet {
            ($($chain: ident = $f: ident),+) => {
                match C::name() {
                    $(
                        ChainName::$chain if evm.$f.is_some() => {
                            let c = evm.$f.clone().unwrap();
                            let pk = c.private_key;
                            let key = SecretKey::from_bytes(pk.as_bytes())?;
                            let wallet =
                                LocalWallet::from(key).with_chain_id(C::chain_id());
                            Ok(wallet)
                        }
                    )+
                    _ => anyhow::bail!("Chain Not Configured!"),
                }
            }
        }

        wallet! {
            Edgeware = edgeware,
            Webb = webb,
            Ganache = ganache,
            Beresheet = beresheet,
            Harmony = harmony
        }
    }

    pub fn fee_percentage<C: EvmChain>(&self) -> anyhow::Result<f64> {
        let evm = &self.config.evm;
        // DRY
        macro_rules! extract_fee_percentage {
            ($($chain: ident = $f: ident),+) => {
                match C::name() {
                    $(
                        ChainName::$chain if evm.$f.is_some() => {
                            let c = evm.$f.clone().unwrap();
                            Ok(c.withdrew_fee_percentage)
                        }
                    )+
                    _ => anyhow::bail!("Chain Fee Not Configured!"),
                }
            }
        }

        extract_fee_percentage! {
            Edgeware = edgeware,
            Webb = webb,
            Ganache = ganache,
            Beresheet = beresheet,
            Harmony = harmony
        }
    }

    pub fn leaves_watcher_enabled<C: EvmChain>(&self) -> bool {
        let evm = &self.config.evm;
        // DRY
        macro_rules! is_enabled {
            ($($chain: ident = $f: ident),+) => {
                match C::name() {
                    $(
                        ChainName::$chain if evm.$f.is_some() => {
                            let c = evm.$f.clone().unwrap();
                            c.enable_leaves_watcher
                        }
                    )+
                    _ => false,
                }
            }
        }

        is_enabled! {
            Edgeware = edgeware,
            Webb = webb,
            Ganache = ganache,
            Beresheet = beresheet,
            Harmony = harmony
        }
    }
}
