use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::tornado::AnchorContract;
use webb::evm::contract::tornado::AnchorContractEvents;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::evm::ethers::contract::LogMeta;

use crate::config;
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;

#[derive(Copy, Clone, Debug)]
pub struct AnchorLeavesWatcher;

#[derive(Clone, Debug)]
pub struct AnchorContractWrapper<M: Middleware> {
    config: config::AnchorContractConfig,
    contract: AnchorContract<M>,
}

impl<M: Middleware> AnchorContractWrapper<M> {
    pub fn new(config: config::AnchorContractConfig, client: Arc<M>) -> Self {
        Self {
            contract: AnchorContract::new(config.common.address, client),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for AnchorContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract for AnchorContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorLeavesWatcher {
    type Middleware = providers::Provider<providers::Http>;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = AnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip(self, store))]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        match event {
            AnchorContractEvents::DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                let value = (leaf_index, H256::from_slice(&commitment));
                store.insert_leaves(contract.address(), &[value])?;
                store.insert_last_deposit_block_number(contract.address(), log.block_number)?;

                tracing::trace!(
                    "Saved Deposit Event ({}, {})",
                    value.0,
                    value.1
                );
            }
            AnchorContractEvents::WithdrawalFilter(_) => {
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

    use crate::config::*;
    use crate::events_watcher::EventWatcher;
    use crate::store::sled::SledStore;
    use crate::test_utils::*;
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn it_should_work() -> anyhow::Result<()> {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .init();
        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(7u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).with_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract_address = deploy_anchor_contract(client.clone()).await?;
        let anchor_contract =
            AnchorContract::new(contract_address, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = StdRng::from_seed([0u8; 32]);
        let config = AnchorContractConfig {
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
        let wrapper = AnchorContractWrapper::new(config, inner_client.clone());
        // make a couple of deposit now, before starting the watcher.
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        let db = tempfile::tempdir()?;
        let store = SledStore::open(db.path())?;
        let store = Arc::new(store.clone());
        // run the leaves watcher in another task
        let task_handle = tokio::task::spawn(AnchorLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper.clone(),
        ));
        // then, make another deposit, while the watcher is running.
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(7)).await;
        // it should now contains the 2 leaves when the watcher was offline, and
        // the new one that happened while it is watching.
        let leaves = store.get_leaves(contract_address)?;
        assert_eq!(expected_leaves, leaves);
        // now let's abort it, and try to do another deposit.
        task_handle.abort();
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        // let's run it again, using the same old store.
        let task_handle = tokio::task::spawn(AnchorLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper.clone(),
        ));
        tracing::debug!("Waiting for 5s allowing the task to run..");
        // let's wait for a bit.. to allow the task to run.
        tokio::time::sleep(Duration::from_secs(5)).await;
        // now it should now contain all the old leaves + the missing one.
        let leaves = store.get_leaves(contract_address)?;
        assert_eq!(expected_leaves, leaves);
        task_handle.abort();
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn last_deposit_block_number_should_update() -> anyhow::Result<()> {
        // Setup deployment of two anchor contracts
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            // .with(fmt::Layer::default().with_writer(file_writer))
            .init();

        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(7u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).with_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract_address_1 = deploy_anchor_contract(client.clone()).await?;
        let contract_address_2 = deploy_anchor_contract(client.clone()).await?;
        let anchor_contract_1 =
            AnchorContract::new(contract_address_1, client.clone());
        let anchor_contract_2 =
            AnchorContract::new(contract_address_2, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = StdRng::from_seed([0u8; 32]);
        let config_1 = AnchorContractConfig {
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
        let config_2 = AnchorContractConfig {
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

        let inner_client = Arc::new(client.provider().clone());
        let wrapper_1 = AnchorContractWrapper::new(config_1, inner_client.clone());
        let wrapper_2 = AnchorContractWrapper::new(config_2, inner_client.clone());
        let db = tempfile::tempdir()?;
        let store = SledStore::open(db.path())?;
        let store = Arc::new(store.clone());

        // start watching for deposits on contract_1
        let task_handle_1 = tokio::task::spawn(AnchorLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper_1.clone(),
        ));
        // start watching for deposits on contract_2
        let task_handle_2 = tokio::task::spawn(AnchorLeavesWatcher.run(
            inner_client.clone(),
            store.clone(),
            wrapper_2.clone(),
        ));

        // make a deposit on each contract to populate last_deposit_block_number
        make_deposit(&mut rng, &anchor_contract_1, &mut expected_leaves).await?;
        make_deposit(&mut rng, &anchor_contract_2, &mut expected_leaves).await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        // get the last_deposit_blocknumer from contract 1
        let expected_deposit_block_number = store.get_last_deposit_block_number(contract_address_1)?;

        println!("first deposit block number");

        // make a deposit on contract 2, which should increase the block_number by 1 for ganache
        make_deposit(&mut rng, &anchor_contract_2, &mut expected_leaves).await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        // check contract 1 last_deposit_block_number is unchanged
        let actual_deposit_block_number = store.get_last_deposit_block_number(contract_address_1)?;
        assert_eq!(expected_deposit_block_number, actual_deposit_block_number);

        // make a deposit on contract 1, which should increase the block_number by 1 for ganache
        make_deposit(&mut rng, &anchor_contract_1, &mut expected_leaves).await?;

        // sleep for the duration of the polling interval
        tokio::time::sleep(Duration::from_secs(2)).await;

        let actual_deposit_block_number = store.get_last_deposit_block_number(contract_address_1)?;
        assert_eq!(expected_deposit_block_number + 2, actual_deposit_block_number);

        task_handle_1.abort();
        task_handle_2.abort();
        Ok(())
    }
}
