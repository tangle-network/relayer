// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
use core::fmt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::path::Path;
use webb::evm::ethers::{self, types};

use crate::store::{BridgeKey, QueueKey};

use super::HistoryStoreKey;
use super::{
    EventHashStore, HistoryStore, LeafCacheStore, ProposalStore, QueueStore,
};
/// SledStore is a store that stores the history of events in  a [Sled](https://sled.rs)-based database.
#[derive(Clone)]
pub struct SledStore {
    db: sled::Db,
}

impl std::fmt::Debug for SledStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SledStore").finish()
    }
}

impl SledStore {
    /// Create a new SledStore.
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = sled::Config::new()
            .path(path)
            .temporary(cfg!(test))
            .use_compression(true)
            .compression_factor(18)
            .open()?;
        Ok(Self { db })
    }
    /// Creates a temporary SledStore.
    pub fn temporary() -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        Self::open(dir.path())
    }
}

impl HistoryStore for SledStore {
    #[tracing::instrument(skip(self))]
    fn set_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        block_number.to_little_endian(&mut bytes);
        let key: HistoryStoreKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }

    #[tracing::instrument(skip(self))]
    fn get_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let key: HistoryStoreKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(default_block_number),
        }
    }
}

impl LeafCacheStore for SledStore {
    type Output = Vec<types::H256>;

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<Self::Output> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        let leaves = tree
            .iter()
            .values()
            .flatten()
            .map(|v| types::H256::from_slice(&v))
            .collect();
        Ok(leaves)
    }

    #[tracing::instrument(skip(self))]
    fn insert_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()> {
        let key: HistoryStoreKey = key.into();

        let tree = self.db.open_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        for (k, v) in leaves {
            tree.insert(k.to_le_bytes(), v.as_bytes())?;
        }
        Ok(())
    }

    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let key: HistoryStoreKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(types::U64::from(0)),
        }
    }

    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        block_number.to_little_endian(&mut bytes);
        let key: HistoryStoreKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }
}

impl EventHashStore for SledStore {
    fn store_event(&self, event: &[u8]) -> anyhow::Result<()> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        tree.insert(hash, &[])?;
        Ok(())
    }

    fn contains_event(&self, event: &[u8]) -> anyhow::Result<bool> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        let exists = tree.contains_key(hash)?;
        Ok(exists)
    }

    fn delete_event(&self, event: &[u8]) -> anyhow::Result<()> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        tree.remove(hash)?;
        Ok(())
    }
}

/// SledQueueKey is a key for a queue in Sled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SledQueueKey {
    EvmTx {
        chain_id: types::U256,
        optional_key: Option<[u8; 64]>,
    },
    BridgeCmd {
        bridge_key: BridgeKey,
    },
}

impl SledQueueKey {
    /// Create a new SledQueueKey from an evm chain id.
    pub fn from_evm_chain_id(chain_id: types::U256) -> Self {
        Self::EvmTx {
            chain_id,
            optional_key: None,
        }
    }
    /// from_evm_with_custom_key returns an EVM specific SledQueueKey.
    pub fn from_evm_with_custom_key(
        chain_id: types::U256,
        key: [u8; 64],
    ) -> Self {
        Self::EvmTx {
            chain_id,
            optional_key: Some(key),
        }
    }

    /// from_bridge_key returns a Bridge specific SledQueueKey.
    pub fn from_bridge_key(bridge_key: BridgeKey) -> Self {
        Self::BridgeCmd { bridge_key }
    }
}

impl fmt::Display for SledQueueKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EvmTx {
                chain_id,
                optional_key,
            } => write!(
                f,
                "EvmTx({}, {:?})",
                chain_id,
                optional_key.map(hex::encode)
            ),
            Self::BridgeCmd { bridge_key } => {
                write!(f, "BridgeCmd({})", bridge_key)
            }
        }
    }
}

impl QueueKey for SledQueueKey {
    fn queue_name(&self) -> String {
        match self {
            Self::EvmTx { chain_id, .. } => format!("evm_tx_{}", chain_id),
            Self::BridgeCmd { bridge_key, .. } => format!(
                "bridge_cmd_{}_{}",
                bridge_key.chain_id.chain_id(),
                hex::encode(bridge_key.target_system.to_bytes())
            ),
        }
    }

    fn item_key(&self) -> Option<[u8; 64]> {
        match self {
            Self::EvmTx { optional_key, .. } => *optional_key,
            Self::BridgeCmd { .. } => None,
        }
    }
}

impl<T> QueueStore<T> for SledStore
where
    T: Serialize + DeserializeOwned + Clone,
{
    type Key = SledQueueKey;

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn enqueue_item(&self, key: Self::Key, item: T) -> anyhow::Result<()> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        let item_bytes = serde_json::to_vec(&item)?;
        // we do everything inside a single transaction
        // so everything happens atomically and if anything fails
        // we revert everything back to the old state.
        tree.transaction::<_, _, std::io::Error>(|db| {
            // get the last id of the queue.
            let last_item_idx = match db.get("last_item_idx")? {
                Some(v) => types::U64::from_big_endian(&v),
                None => types::U64::zero(),
            };
            // increment it.
            let next_idx = last_item_idx + types::U64::one();
            let mut idx_bytes = [0u8; std::mem::size_of::<types::U64>()];
            next_idx.to_big_endian(&mut idx_bytes);
            // then save it.
            db.insert("last_item_idx", &idx_bytes)?;
            db.insert("key_prefix", "item")?;
            // we create a item key like so
            // tx_key = 4 bytes prefix ("item") + 8 bytes of the index.
            let mut item_key = [0u8; 4 + std::mem::size_of::<types::U64>()];
            let prefix =
                db.get("key_prefix")?.unwrap_or_else(|| b"item".into());
            item_key[0..4].copy_from_slice(&prefix);
            item_key[4..].copy_from_slice(&idx_bytes);
            // then we save it.
            db.insert(&item_key, item_bytes.as_slice())?;
            if let Some(k) = key.item_key() {
                // also save the key where we can find it by special key.
                db.insert(&k[..], &item_key)?;
            }
            tracing::trace!("enqueue item under key = {}", key);
            Ok(())
        })?;
        // flush the db to make sure we don't lose anything.
        self.db.flush()?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn dequeue_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        // now we create a lazy iterator that will scan
        // over all saved items in the queue
        // with the specific key prefix.
        let prefix = tree.get("key_prefix")?.unwrap_or_else(|| b"item".into());
        let mut queue = tree.scan_prefix(prefix);
        let (key, value) = match queue.next() {
            Some(Ok(v)) => v,
            _ => {
                tracing::trace!("queue is empty ..");
                return Ok(None);
            }
        };
        let item = serde_json::from_slice(&value)?;
        // now it is safe to remove it from the queue.
        tree.remove(key)?;
        // flush db
        self.db.flush()?;
        Ok(Some(item))
    }

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn peek_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        // this method, is similar to dequeue_tx, expect we don't
        // remove anything from the queue.
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        let prefix = tree.get("key_prefix")?.unwrap_or_else(|| b"item".into());
        let mut queue = tree.scan_prefix(prefix);
        let (_, value) = match queue.next() {
            Some(Ok(v)) => v,
            _ => return Ok(None),
        };
        let item = serde_json::from_slice(&value)?;
        Ok(Some(item))
    }

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn has_item(&self, key: Self::Key) -> anyhow::Result<bool> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        if let Some(k) = key.item_key() {
            tree.contains_key(&k[..]).map_err(Into::into)
        } else {
            Ok(false)
        }
    }

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn remove_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        let inner_key = match key.item_key() {
            Some(k) => k,
            None => return Ok(None),
        };
        match tree.get(&inner_key[..])? {
            Some(k) => {
                let exists = tree.remove(&k)?;
                tree.remove(&inner_key)?;
                let item = exists.and_then(|v| serde_json::from_slice(&v).ok());
                tracing::trace!("removed item from the queue..");
                self.db.flush()?;
                Ok(item)
            }
            None => {
                // not found!
                anyhow::bail!("item with key {} not found in queue", key);
            }
        }
    }
}

impl ProposalStore for SledStore {
    type Proposal = ();

    #[tracing::instrument(skip_all)]
    fn insert_proposal(&self, proposal: Self::Proposal) -> anyhow::Result<()> {
        let tree = self.db.open_tree("proposal_store")?;
        tree.insert(&"TODO", serde_json::to_vec(&proposal)?.as_slice())?;
        Ok(())
    }

    #[tracing::instrument(
        skip_all,
        fields(data_hash = %hex::encode(data_hash))
    )]
    fn remove_proposal(
        &self,
        data_hash: &[u8],
    ) -> anyhow::Result<Option<Self::Proposal>> {
        let tree = self.db.open_tree("proposal_store")?;
        match tree.get(&data_hash)? {
            Some(bytes) => {
                let proposal: Self::Proposal = serde_json::from_slice(&bytes)?;
                Ok(Some(proposal))
            }
            None => {
                tracing::warn!(
                    "Proposal not seen yet; not found in the proposal storage."
                );
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_types::Address;
    use webb::evm::contract::protocol_solidity::WithdrawalFilter;
    use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
    use webb::evm::ethers::types::transaction::request::TransactionRequest;

    impl SledQueueKey {
        pub fn from_evm_tx(
            chain_id: types::U256,
            tx: &TypedTransaction,
        ) -> Self {
            let key = {
                let mut bytes = [0u8; 64];
                let tx_hash = tx.sighash(chain_id.as_u64());
                bytes[..32].copy_from_slice(tx_hash.as_fixed_bytes());
                bytes
            };
            Self::from_evm_with_custom_key(chain_id, key)
        }
    }

    #[test]
    fn get_leaves_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = types::U256::one();
        let contract =
            types::H160::from_slice("11111111111111111111".as_bytes());
        let block_number = types::U64::from(20);
        let default_block_number = types::U64::from(1);

        store
            .set_last_block_number((chain_id, contract), block_number)
            .unwrap();

        let block = store
            .get_last_block_number((contract, chain_id), default_block_number);

        match block {
            Ok(b) => {
                println!("retrieved block {:?}", block);
                assert_eq!(b, types::U64::from(20));
            }
            Err(e) => {
                println!("Error encountered {:?}", e);
            }
        }
    }

    #[test]
    fn tx_queue_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = types::U256::one();
        // it is now empty
        assert_eq!(
            store
                .dequeue_item(SledQueueKey::from_evm_chain_id(chain_id))
                .unwrap(),
            Option::<TypedTransaction>::None
        );

        let tx1: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store
            .enqueue_item(
                SledQueueKey::from_evm_tx(chain_id, &tx1),
                tx1.clone(),
            )
            .unwrap();
        let tx2: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store
            .enqueue_item(
                SledQueueKey::from_evm_tx(chain_id, &tx2),
                tx2.clone(),
            )
            .unwrap();

        // now let's dequeue transactions.
        assert_eq!(
            store
                .dequeue_item(SledQueueKey::from_evm_chain_id(chain_id))
                .unwrap(),
            Some(tx1)
        );
        assert_eq!(
            store
                .dequeue_item(SledQueueKey::from_evm_chain_id(chain_id))
                .unwrap(),
            Some(tx2)
        );

        let tx3: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store
            .enqueue_item(
                SledQueueKey::from_evm_tx(chain_id, &tx3),
                tx3.clone(),
            )
            .unwrap();
        assert_eq!(
            store
                .peek_item(SledQueueKey::from_evm_chain_id(chain_id))
                .unwrap(),
            Some(tx3.clone())
        );
        assert!(QueueStore::<TypedTransaction>::has_item(
            &store,
            SledQueueKey::from_evm_tx(chain_id, &tx3)
        )
        .unwrap());
        let _: Option<TypedTransaction> = store
            .remove_item(SledQueueKey::from_evm_tx(chain_id, &tx3))
            .unwrap();
        assert!(!QueueStore::<TypedTransaction>::has_item(
            &store,
            SledQueueKey::from_evm_tx(chain_id, &tx3)
        )
        .unwrap());
        assert_eq!(
            store
                .dequeue_item(SledQueueKey::from_evm_chain_id(chain_id))
                .unwrap(),
            Option::<TypedTransaction>::None
        );
    }

    #[test]
    fn events_hash_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();

        let events = (0..20)
            .map(|_| WithdrawalFilter {
                to: Address::random(),
                relayer: Address::random(),
                fee: types::U256::zero(),
            })
            .collect::<Vec<_>>();

        for event in &events {
            let event_bytes = serde_json::to_vec(&event).unwrap();
            // check if the event is already in the store
            assert!(!store.contains_event(&event_bytes).unwrap());
            // add the event
            store.store_event(&event_bytes).unwrap();
        }

        for event in &events {
            let event_bytes = serde_json::to_vec(&event).unwrap();
            // check if the event is in the store
            assert!(store.contains_event(&event_bytes).unwrap());
            // remove the event
            store.delete_event(&event_bytes).unwrap();
        }
    }
}
