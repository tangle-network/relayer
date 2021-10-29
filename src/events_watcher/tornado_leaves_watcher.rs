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

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use tracing_test::traced_test;

    use super::*;
    use crate::config::*;
    use crate::events_watcher::EventWatcher;
    use crate::store::sled::SledStore;
    use crate::test_utils::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[traced_test]
    async fn it_should_work() -> anyhow::Result<()> {
        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(7u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).with_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract_address = deploy_tornado_contract(client.clone()).await?;
        let tornado_contract =
            TornadoContract::new(contract_address, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = StdRng::from_seed([0u8; 32]);
        let config = TornadoContractConfig {
            common: CommonContractConfig {
                address: contract_address,
                deployed_at: 2,
            },
            events_watcher: EventsWatcherConfig {
                enabled: true,
                polling_interval: 1000,
            },
            size: 1.0,
            withdraw_config: AnchorWithdrawConfig {
                withdraw_fee_percentage: 0.0000000001,
                withdraw_gaslimit: 0.into(),
            },
        };

        let inner_client = Arc::new(client.provider().clone());
        let wrapper = TornadoContractWrapper::new(config, inner_client.clone());
        // make a couple of deposit now, before starting the watcher.
        make_deposit(&mut rng, &tornado_contract, &mut expected_leaves).await?;
        make_deposit(&mut rng, &tornado_contract, &mut expected_leaves).await?;
        let db = tempfile::tempdir()?;
        let store = SledStore::open(db.path())?;
        let store = Arc::new(store.clone());
        // run the leaves watcher in another task
        let task_handle = tokio::task::spawn(TornadoLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper.clone(),
        ));
        // then, make another deposit, while the watcher is running.
        make_deposit(&mut rng, &tornado_contract, &mut expected_leaves).await?;
        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;
        // it should now contains the 2 leaves when the watcher was offline, and
        // the new one that happened while it is watching.
        let chain_id = inner_client.get_chainid().await?;
        let leaves = store.get_leaves((chain_id, contract_address))?;
        assert_eq!(expected_leaves, leaves);
        // now let's abort it, and try to do another deposit.
        task_handle.abort();
        make_deposit(&mut rng, &tornado_contract, &mut expected_leaves).await?;
        // let's run it again, using the same old store.
        let task_handle = tokio::task::spawn(TornadoLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper.clone(),
        ));
        tracing::debug!("Waiting for 5s allowing the task to run..");
        // let's wait for a bit.. to allow the task to run.
        tokio::time::sleep(Duration::from_secs(5)).await;
        // now it should now contain all the old leaves + the missing one.
        let leaves = store.get_leaves((chain_id, contract_address))?;
        assert_eq!(expected_leaves, leaves);
        task_handle.abort();
        drop(ganache);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[traced_test]
    async fn last_deposit_block_number_should_update() -> anyhow::Result<()> {
        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(7u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).with_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract_address_1 =
            deploy_tornado_contract(client.clone()).await?;
        let contract_address_2 =
            deploy_tornado_contract(client.clone()).await?;
        let tornado_contract_1 =
            TornadoContract::new(contract_address_1, client.clone());
        let tornado_contract_2 =
            TornadoContract::new(contract_address_2, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = StdRng::from_seed([0u8; 32]);
        let config_1 = TornadoContractConfig {
            common: CommonContractConfig {
                address: contract_address_1,
                deployed_at: 2,
            },
            events_watcher: EventsWatcherConfig {
                enabled: true,
                polling_interval: 1000,
            },
            size: 1.0,
            withdraw_config: AnchorWithdrawConfig {
                withdraw_fee_percentage: 0.0000000001,
                withdraw_gaslimit: 0.into(),
            },
        };
        let config_2 = TornadoContractConfig {
            common: CommonContractConfig {
                address: contract_address_2,
                deployed_at: 3,
            },
            events_watcher: EventsWatcherConfig {
                enabled: true,
                polling_interval: 1000,
            },
            size: 1.0,
            withdraw_config: AnchorWithdrawConfig {
                withdraw_fee_percentage: 0.0000000001,
                withdraw_gaslimit: 0.into(),
            },
        };

        let inner_client_1 = Arc::new(client.provider().clone());
        let inner_client_2 = Arc::new(client.provider().clone());
        let wrapper_1 =
            TornadoContractWrapper::new(config_1, inner_client_1.clone());
        let wrapper_2 =
            TornadoContractWrapper::new(config_2, inner_client_2.clone());
        let db = tempfile::tempdir()?;
        let store = SledStore::open(db.path())?;
        let store = Arc::new(store.clone());
        let chain_id = inner_client_1.get_chainid().await?;

        // start watching for deposits on contract_1
        let task_handle_1 = tokio::task::spawn(TornadoLeavesWatcher.run(
            inner_client_1.clone(),
            store.clone(),
            wrapper_1.clone(),
        ));
        // start watching for deposits on contract_2
        let task_handle_2 = tokio::task::spawn(TornadoLeavesWatcher.run(
            inner_client_2.clone(),
            store.clone(),
            wrapper_2.clone(),
        ));

        // make a deposit on each contract to populate last_deposit_block_number
        make_deposit(&mut rng, &tornado_contract_2, &mut expected_leaves)
            .await?;
        make_deposit(&mut rng, &tornado_contract_1, &mut expected_leaves)
            .await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        // get the last_deposit_blocknumer from contract 1
        let expected_deposit_block_number = store
            .get_last_deposit_block_number((chain_id, contract_address_1))?;

        // make a deposit on contract 2, which should increase the block_number by 1 for ganache
        make_deposit(&mut rng, &tornado_contract_2, &mut expected_leaves)
            .await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        // check contract 1 last_deposit_block_number is unchanged
        let actual_deposit_block_number = store
            .get_last_deposit_block_number((chain_id, contract_address_1))?;
        assert_eq!(expected_deposit_block_number, actual_deposit_block_number);

        // make a deposit on contract 1, which should increase the block_number by 1 for ganache
        make_deposit(&mut rng, &tornado_contract_1, &mut expected_leaves)
            .await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        let actual_deposit_block_number = store
            .get_last_deposit_block_number((chain_id, contract_address_1))?;
        assert_eq!(
            expected_deposit_block_number + 2,
            actual_deposit_block_number
        );

        task_handle_1.abort();
        task_handle_2.abort();
        drop(ganache);
        Ok(())
    }
}
