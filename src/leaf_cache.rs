use std::fmt::{self, Debug};
use std::path::Path;
use std::sync::Arc;

use anyhow::Error;
use ethers::prelude::*;
use futures::prelude::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::Duration;
use tracing::Instrument;
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

    /// Sets the new block number for that contract in the cache and returns the old one.
    fn set_last_block_number(
        &self,
        contract: Address,
        block_number: U64,
    ) -> anyhow::Result<U64>;

    fn get_last_block_number(
        &self,
        contract: Address,
        default_block_number: U64,
    ) -> anyhow::Result<U64>;

    fn get_last_block_number_or_default(
        &self,
        contract: Address,
    ) -> anyhow::Result<U64> {
        self.get_last_block_number(contract, U64::one())
    }
}

type MemStore = HashMap<Address, Vec<(u32, H256)>>;

#[derive(Clone, Default)]
pub struct InMemoryLeafCache {
    store: Arc<RwLock<MemStore>>,
    last_block_numbers: Arc<RwLock<HashMap<Address, U64>>>,
}

impl Debug for InMemoryLeafCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("InMemoryLeafCache")
    }
}

impl LeafCacheStore for InMemoryLeafCache {
    type Output = Vec<H256>;

    #[tracing::instrument(skip(self))]
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

    #[tracing::instrument(skip(self))]
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

    #[tracing::instrument(skip(self))]
    fn get_last_block_number(
        &self,
        contract: Address,
        default_block_number: U64,
    ) -> anyhow::Result<U64> {
        let guard = self.last_block_numbers.read();
        let val = guard
            .get(&contract)
            .cloned()
            .unwrap_or(default_block_number);
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn set_last_block_number(
        &self,
        contract: Address,
        block_number: U64,
    ) -> anyhow::Result<U64> {
        let mut guard = self.last_block_numbers.write();
        let val = guard.entry(contract).or_insert(block_number);
        let old = *val;
        *val = block_number;
        Ok(old)
    }
}

#[derive(Clone)]
pub struct SledLeafCache {
    db: sled::Db,
}

impl Debug for SledLeafCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("SledLeafCache")
    }
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

    #[tracing::instrument]
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

    #[tracing::instrument]
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

    #[tracing::instrument]
    fn set_last_block_number(
        &self,
        contract: Address,
        block_number: U64,
    ) -> anyhow::Result<U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let mut bytes = [0u8; std::mem::size_of::<u64>()];
        block_number.to_little_endian(&mut bytes);
        let old = tree.insert(contract, &bytes)?;
        match old {
            Some(v) => Ok(U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }

    #[tracing::instrument]
    fn get_last_block_number(
        &self,
        contract: Address,
        default_block_number: U64,
    ) -> anyhow::Result<U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let val = tree.get(contract)?;
        match val {
            Some(v) => Ok(U64::from_little_endian(&v)),
            None => Ok(default_block_number),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LeavesWatcher<S> {
    ws_endpoint: String,
    store: S,
    contract: Address,
    start_block_number: u64,
}

impl<S> LeavesWatcher<S>
where
    S: LeafCacheStore + Debug + Clone + Send + Sync + 'static,
{
    pub fn new(
        ws_endpoint: impl Into<String>,
        store: S,
        contract: Address,
        start_block_number: u64,
    ) -> Self {
        Self {
            ws_endpoint: ws_endpoint.into(),
            contract,
            store,
            start_block_number,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn run(self) -> anyhow::Result<()> {
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async { self.watch().await };
        backoff::future::retry(backoff, task).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(contract = %self.contract))]
    async fn watch(&self) -> anyhow::Result<(), backoff::Error<Error>> {
        tracing::trace!("Connecting to {}", self.ws_endpoint);
        let endpoint = url::Url::parse(&self.ws_endpoint)
            .map_err(Error::from)
            .map_err(backoff::Error::Permanent)?;
        let ws = Ws::connect(endpoint)
            .map_err(Error::from)
            .instrument(tracing::trace_span!("websocket"))
            .await?;
        let fetch_interval = Duration::from_millis(200);
        let provider = Provider::new(ws).interval(fetch_interval);
        let client = Arc::new(provider);

        self.fetch_previous_deposits(client.clone()).await?;
        self.poll_for_events(client.clone()).await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(contract = %self.contract))]
    async fn fetch_previous_deposits(&self, client: Arc<Provider<Ws>>) -> anyhow::Result<(), backoff::Error<Error>> {
        let contract = AnchorContract::new(self.contract, client.clone());
        let mut block = self.store.get_last_block_number(
            contract.address(),
            self.start_block_number.into(),
        )?;
        tracing::trace!("Starting from block {}", block + 1);
        let current_block_number =
            client.get_block_number().map_err(Error::from).await?;
        tracing::trace!("And Last Block is #{}", current_block_number);
        let step = U64::from(50);
        while block < current_block_number {
            let dest_block = std::cmp::min(block + step, current_block_number);
            tracing::trace!("Reading from #{} to #{}", block + 1, dest_block);
            let filter = contract
                .deposit_filter()
                .from_block(block + 1)
                .to_block(dest_block);
            let missing_events = filter
                .query_with_meta()
                .instrument(tracing::trace_span!("query_with_meta"))
                .map_err(Error::from)
                .await?;
            tracing::trace!("Got #{} missing events", missing_events.len());
            if missing_events.is_empty() {
                // if no missing events in this region
                // we move on.
                self.store
                    .set_last_block_number(contract.address(), dest_block)?;
            }
            for (e, log) in missing_events {
                self.store.insert_leaves(
                    contract.address(),
                    &[(e.leaf_index, H256::from_slice(&e.commitment))],
                )?;
                let old = self.store.set_last_block_number(
                    contract.address(),
                    log.block_number,
                )?;
                tracing::trace!("Going from #{} to #{}", old, log.block_number);
            }

            block = self.store.get_last_block_number(
                contract.address(),
                self.start_block_number.into(),
            )?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(contract = %self.contract))]
    async fn poll_for_events(&self, client: Arc<Provider<Ws>>) -> Result<(), backoff::Error<Error>> {
        let mut block = self.store.get_last_block_number(
            self.contract,
            self.start_block_number.into(),
        )?;
        let contract = AnchorContract::new(self.contract, client.clone());

        // now we start polling for new events.
        loop {
            let current_block_number = client.get_block_number().map_err(Error::from).await?;
            let events_filter = contract.deposit_filter().from_block(block).to_block(current_block_number);
            let found_events = events_filter.query_with_meta().map_err(Error::from).await?;

            for (e, _log) in found_events {
                self.store.insert_leaves(
                    contract.address(),
                    &[(e.leaf_index, H256::from_slice(&e.commitment))],
                )?;
            }

            tracing::trace!("Polled from #{} to #{}", block, current_block_number);
            self.store
                    .set_last_block_number(contract.address(), current_block_number)?;

            block = current_block_number;

            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(contract = %self.contract))]
    async fn stream_for_events(&self, client: Arc<Provider<Ws>>) -> Result<(), backoff::Error<Error>> {
        
        let contract = AnchorContract::new(self.contract, client.clone());
        let block = self.store.get_last_block_number(
            self.contract,
            self.start_block_number.into(),
        )?;

        // Start listening to stream for new events.
        let events_filter = contract.deposit_filter().from_block(block);
        let events = events_filter.subscribe().map_err(Error::from).await?;
        let mut stream = events.with_meta();
        loop {
            let maybe_event = stream.try_next().map_err(Error::from).await?;
            match maybe_event {
                Some(v) => {
                    let (e, log) = v;
                    self.store.insert_leaves(
                        contract.address(),
                        &[(e.leaf_index, H256::from_slice(&e.commitment))],
                    )?;
                    tracing::trace!("Got new Event at block {}", log.block_number);
                    self.store
                        .set_last_block_number(contract.address(), log.block_number)?;
                },
                None => {
                    let e = backoff::Error::Transient(anyhow::anyhow!("Reconnect"));
                    tracing::warn!("Connection Dropped from {}", self.ws_endpoint);
                    tracing::info!("Restarting the task for {}", self.contract);
                    return Err(e);
                }
            }
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
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_test_writer()
            .init();
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
            0,
        );
        // run the leaves watcher in another task
        let task_handle = tokio::task::spawn(leaves_watcher.run());
        // then, make another deposit, while the watcher is running.
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        tokio::time::sleep(Duration::from_secs(5)).await;
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
            0,
        );
        let task_handle = tokio::task::spawn(leaves_watcher.run());
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
