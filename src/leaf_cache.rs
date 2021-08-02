use std::path::Path;
use std::sync::Arc;

use anyhow::Error;
use ethers::prelude::*;
use futures::prelude::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use webb::evm::contract::anchor::AnchorContract;
use webb::evm::ethers;

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore {
    type Output: IntoIterator<Item = H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output>;

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[(u32, H256)],
    ) -> anyhow::Result<()>;
    /// Sets the new block number for the cache and returns the old one.
    fn set_last_block_number(&self, block_number: U64) -> anyhow::Result<U64>;
    fn get_last_block_number(&self) -> anyhow::Result<U64>;
}

type MemStore = HashMap<Address, Vec<(u32, H256)>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryLeafCache {
    store: Arc<RwLock<MemStore>>,
    last_block_number: Arc<AtomicU64>,
}

impl LeafCacheStore for InMemoryLeafCache {
    type Output = Vec<H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output> {
        let guard = self.store.read();
        let val = guard
            .get(&contract)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.1)
            .collect();
        Ok(val)
    }

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[(u32, H256)],
    ) -> anyhow::Result<()> {
        let mut guard = self.store.write();
        guard
            .entry(contract)
            .and_modify(|v| v.extend_from_slice(leaves))
            .or_insert_with(|| leaves.to_vec());
        Ok(())
    }

    fn get_last_block_number(&self) -> anyhow::Result<U64> {
        let val = self.last_block_number.load(Ordering::Relaxed);
        Ok(U64::from(val))
    }

    fn set_last_block_number(&self, block_number: U64) -> anyhow::Result<U64> {
        let old = self
            .last_block_number
            .swap(block_number.as_u64(), Ordering::Relaxed);
        Ok(U64::from(old))
    }
}

#[derive(Debug, Clone)]
pub struct SledLeafCache {
    db: sled::Db,
}

impl SledLeafCache {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = sled::Config::new()
            .path(path)
            .temporary(cfg!(test))
            .use_compression(true)
            .compression_factor(18)
            .open()?;
        Ok(Self { db })
    }
}

impl LeafCacheStore for SledLeafCache {
    type Output = Vec<H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output> {
        let tree = self.db.open_tree(contract)?;
        let leaves = tree
            .iter()
            .values()
            .flatten()
            .map(|v| H256::from_slice(&v))
            .collect();
        Ok(leaves)
    }

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[(u32, H256)],
    ) -> anyhow::Result<()> {
        let tree = self.db.open_tree(contract)?;
        for (k, v) in leaves {
            tree.insert(k.to_le_bytes(), v.as_bytes())?;
        }
        Ok(())
    }

    fn set_last_block_number(&self, block_number: U64) -> anyhow::Result<U64> {
        let mut bytes = [0u8; std::mem::size_of::<u64>()];
        block_number.to_little_endian(&mut bytes);
        let old = self.db.insert(b"last_block_number", &bytes)?;
        match old {
            Some(v) => Ok(U64::from_little_endian(&v)),
            None => Ok(U64::zero()),
        }
    }

    fn get_last_block_number(&self) -> anyhow::Result<U64> {
        let val = self.db.get(b"last_block_number")?;
        match val {
            Some(v) => Ok(U64::from_little_endian(&v)),
            None => Ok(U64::zero()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LeavesWatcher<S> {
    ws_endpoint: String,
    store: S,
    contract: Address,
}

impl<S> LeavesWatcher<S>
where
    S: LeafCacheStore + Clone + Send + Sync + 'static,
{
    pub fn new(
        ws_endpoint: impl Into<String>,
        store: S,
        contract: Address,
    ) -> Self {
        Self {
            ws_endpoint: ws_endpoint.into(),
            contract,
            store,
        }
    }

    pub async fn watch(self) -> anyhow::Result<()> {
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async { self.watch_for_events().await };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }

    async fn watch_for_events(&self) -> Result<(), backoff::Error<Error>> {
        log::debug!("Connecting to {}", self.ws_endpoint);
        let endpoint = url::Url::parse(&self.ws_endpoint)
            .map_err(Error::from)
            .map_err(backoff::Error::Permanent)?;
        let ws = Ws::connect(endpoint).map_err(Error::from).await?;
        let fetch_interval = Duration::from_millis(200);
        let provider = Provider::new(ws).interval(fetch_interval);
        let client = Arc::new(provider);
        let contract = AnchorContract::new(self.contract, client.clone());
        let block = self.store.get_last_block_number()?;
        log::debug!("Starting from block {}", block + 1);
        let filter = contract.deposit_filter().from_block(block + 1);
        let missing_events =
            filter.query_with_meta().map_err(Error::from).await?;
        log::debug!("Got #{} missing events", missing_events.len());
        for (e, log) in missing_events {
            self.store.insert_leaves(
                contract.address(),
                &[(e.leaf_index, H256::from_slice(&e.commitment))],
            )?;
            let old = self.store.set_last_block_number(log.block_number)?;
            log::debug!("Going from #{} to #{}", old, log.block_number);
        }
        let events = filter.subscribe().map_err(Error::from).await?;
        let mut stream = events.with_meta();
        while let Some(v) = stream.try_next().map_err(Error::from).await? {
            let (e, log) = v;
            self.store.insert_leaves(
                contract.address(),
                &[(e.leaf_index, H256::from_slice(&e.commitment))],
            )?;
            self.store.set_last_block_number(log.block_number)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use crate::test_utils::*;

    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn watcher() -> anyhow::Result<()> {
        env_logger::builder().is_test(true).init();
        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(10u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).with_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract_address = deploy_anchor_contract(client.clone()).await?;
        let contract = AnchorContract::new(contract_address, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = StdRng::from_seed([0u8; 32]);
        // make a couple of deposit now, before starting the watcher.
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        let db = tempfile::tempdir()?;
        let store = SledLeafCache::open(db.path())?;
        let leaves_watcher = LeavesWatcher::new(
            ganache.ws_endpoint(),
            store.clone(),
            contract_address,
        );
        // run the leaves watcher in another task
        let task_handle = tokio::task::spawn(leaves_watcher.watch());
        // then, make another deposit, while the watcher is running.
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        // it should now contains the 2 leaves when the watcher was offline, and
        // the new one that happened while it is watching.
        let leaves = store.get_leaves(contract_address)?;
        assert_eq!(expected_leaves, leaves);
        // now let's abort it, and try to do another deposit.
        task_handle.abort();
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        // let's run it again, using the same old store.
        let leaves_watcher = LeavesWatcher::new(
            ganache.ws_endpoint(),
            store.clone(),
            contract_address,
        );
        let task_handle = tokio::task::spawn(leaves_watcher.watch());
        log::debug!("Waiting for 5s allowing the task to run..");
        // let's wait for a bit.. to allow the task to run.
        tokio::time::sleep(Duration::from_secs(5)).await;
        // now it should now contain all the old leaves + the missing one.
        let leaves = store.get_leaves(contract_address)?;
        assert_eq!(expected_leaves, leaves);
        task_handle.abort();
        Ok(())
    }
}
