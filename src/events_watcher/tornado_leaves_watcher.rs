use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::tornado::TornadoContract;
use webb::evm::contract::tornado::TornadoContractEvents;
use webb::evm::ethers::contract::LogMeta;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;

use crate::config;
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;

#[derive(Copy, Clone, Debug)]
pub struct TornadoLeavesWatcher;

#[derive(Clone, Debug)]
pub struct TornadoContractWrapper<M: Middleware> {
    config: config::TornadoContractConfig,
    contract: TornadoContract<M>,
}

impl<M: Middleware> TornadoContractWrapper<M> {
    pub fn new(config: config::TornadoContractConfig, client: Arc<M>) -> Self {
        Self {
            contract: TornadoContract::new(config.common.address, client),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for TornadoContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract for TornadoContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }

    fn max_events_per_step(&self) -> types::U64 {
        self.config.events_watcher.max_events_per_step.into()
    }

    fn print_progress_interval(&self) -> Duration {
        Duration::from_millis(
            self.config.events_watcher.print_progress_interval,
        )
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for TornadoLeavesWatcher {
    const TAG: &'static str = "Tornado Watcher For Leaves";

    type Middleware = providers::Provider<providers::Http>;

    type Contract = TornadoContractWrapper<Self::Middleware>;

    type Events = TornadoContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        match event {
            TornadoContractEvents::DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                let value = (leaf_index, H256::from_slice(&commitment));
                let chain_id = contract.client().get_chainid().await?;
                store
                    .insert_leaves((chain_id, contract.address()), &[value])?;
                store.insert_last_deposit_block_number(
                    (chain_id, contract.address()),
                    log.block_number,
                )?;

                tracing::debug!(
                    "Saved Deposit Event ({}, {})",
                    value.0,
                    value.1
                );
            }
            TornadoContractEvents::WithdrawalFilter(_) => {
                // we don't care for withdraw events for now
            }
        };

        Ok(())
    }
}
