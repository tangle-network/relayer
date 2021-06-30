use once_cell::sync::OnceCell;
use webb::evm::ethers::prelude::*;

use crate::chains::evm::EvmChain;
use crate::config;

#[derive(Debug, Clone)]
pub struct LateInit<T> {
    cell: OnceCell<T>,
}

impl<T> LateInit<T> {
    pub fn init(&self, value: T) { assert!(self.cell.set(value).is_ok()) }
}

impl<T> Default for LateInit<T> {
    fn default() -> Self {
        LateInit {
            cell: OnceCell::default(),
        }
    }
}

impl<T> std::ops::Deref for LateInit<T> {
    type Target = T;

    fn deref(&self) -> &T { self.cell.get().unwrap() }
}

#[derive(Clone)]
pub struct RelayerContext {
    config: config::WebbRelayerConfig,
}

impl RelayerContext {
    pub fn new(config: config::WebbRelayerConfig) -> Self { Self { config } }

    pub async fn evm_provider<C: EvmChain>(
        &self,
    ) -> anyhow::Result<Provider<Http>> {
        todo!()
    }

    pub async fn evm_wallet<C: EvmChain>(&self) -> anyhow::Result<LocalWallet> {
        todo!()
    }
}
