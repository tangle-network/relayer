use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::anchor::AnchorContract;
use webb::evm::contract::anchor::AnchorContractEvents;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;

use crate::config;
use crate::store::sled::SledLeafCache;
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
        Duration::from_millis(self.config.leaves_watcher.polling_interval)
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorLeavesWatcher {
    type Middleware = providers::Provider<providers::Http>;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = AnchorContractEvents;

    type Store = SledLeafCache;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract_address: types::Address,
        event: Self::Events,
    ) -> anyhow::Result<()> {
        match event {
            AnchorContractEvents::DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                store.insert_leaves(
                    contract_address,
                    &[(leaf_index, H256::from_slice(&commitment))],
                )?;
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
    use crate::store::sled::SledLeafCache;
    use crate::test_utils::*;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn watcher() -> anyhow::Result<()> {
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
            leaves_watcher: AnchorLeavesWatcherConfig {
                enabled: true,
                polling_interval: 1000,
            },
            size: 1.0,
        };

        let inner_client = Arc::new(client.provider().clone());
        let wrapper = AnchorContractWrapper::new(config, inner_client.clone());
        // make a couple of deposit now, before starting the watcher.
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        make_deposit(&mut rng, &anchor_contract, &mut expected_leaves).await?;
        let db = tempfile::tempdir()?;
        let store = SledLeafCache::open(db.path())?;
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
}
