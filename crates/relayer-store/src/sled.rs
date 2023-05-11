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

use crate::{BridgeKey, QueueKey};
use core::fmt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::Path;
use webb::evm::ethers::{self, types};

use super::HistoryStoreKey;
use super::{
    EncryptedOutputCacheStore, EventHashStore, HistoryStore, LeafCacheStore,
    ProposalStore, QueueStore, TokenPriceCacheStore,
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
    pub fn open<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let db = sled::Config::new()
            .path(path)
            .temporary(cfg!(test))
            .mode(sled::Mode::HighThroughput)
            .open()?;
        Ok(Self { db })
    }
    /// Creates a temporary SledStore.
    pub fn temporary() -> crate::Result<Self> {
        let dir = tempfile::tempdir()?;
        Self::open(dir.path())
    }

    /// Gets the total amount of data stored on disk
    pub fn get_data_stored_size(&self) -> u64 {
        self.db.size_on_disk().unwrap_or_default()
    }
}

impl HistoryStore for SledStore {
    #[tracing::instrument(skip(self))]
    fn set_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let bytes = block_number.to_le_bytes();
        let key: HistoryStoreKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_be_bytes(output))
            }
            None => Ok(block_number),
        }
    }

    #[tracing::instrument(skip(self))]
    fn get_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: u64,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let key: HistoryStoreKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_le_bytes(output))
            }
            None => Ok(default_block_number),
        }
    }
}

impl LeafCacheStore for SledStore {
    type Output = BTreeMap<u32, types::H256>;

    #[tracing::instrument(skip(self))]
    fn clear_leaves_cache<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<()> {
        let key: HistoryStoreKey = key.into();
        self.db.drop_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        let leaves_map: BTreeMap<_, _> = tree
            .iter()
            .flatten()
            .map(|(k, v)| {
                let leaf_index_bytes = k.get(0..4).expect("leaf index bytes");
                let leaf_index_bytes = leaf_index_bytes
                    .try_into()
                    .expect("leaf index bytes is u32 bytes");
                let leaf_index = u32::from_le_bytes(leaf_index_bytes);
                let leaf = types::H256::from_slice(&v);
                (leaf_index, leaf)
            })
            .collect();
        Ok(leaves_map)
    }

    #[tracing::instrument(skip(self))]
    fn get_leaves_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        let range_start = range.start.to_le_bytes();
        let range_end = range.end.to_le_bytes();
        let leaves = tree
            .range(range_start..range_end)
            .flatten()
            .map(|(k, v)| {
                let leaf_index_bytes = k.get(0..4).expect("leaf index bytes");
                let leaf_index_bytes = leaf_index_bytes
                    .try_into()
                    .expect("leaf index bytes is u32 bytes");
                let leaf_index = u32::from_le_bytes(leaf_index_bytes);
                let leaf = types::H256::from_slice(&v);
                (leaf_index, leaf)
            })
            .collect();
        Ok(leaves)
    }
    #[tracing::instrument(skip(self))]
    fn insert_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, Vec<u8>)],
    ) -> crate::Result<()> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "leaves/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        tree.transaction(|db| {
            for (k, v) in leaves {
                db.insert(&k.to_le_bytes(), v.as_slice())?;
            }
            Ok(())
        })?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let key: HistoryStoreKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_le_bytes(output))
            }
            None => Ok(0u64),
        }
    }

    #[tracing::instrument(skip(self))]
    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let bytes = block_number.to_le_bytes();
        let key: HistoryStoreKey = key.into();
        let old = tree.transaction(|db| {
            let old = db.insert(key.to_bytes(), &bytes)?;
            Ok(old)
        })?;
        match old {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_le_bytes(output))
            }
            None => Ok(block_number),
        }
    }
}

impl EncryptedOutputCacheStore for SledStore {
    type Output = Vec<Vec<u8>>;

    #[tracing::instrument(skip(self))]
    fn get_encrypted_output<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "encrypted_outputs/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        let encrypted_outputs: Vec<_> =
            tree.iter().values().flatten().map(|v| v.to_vec()).collect();
        Ok(encrypted_outputs)
    }

    #[tracing::instrument(skip(self))]
    fn get_encrypted_output_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output> {
        let key: HistoryStoreKey = key.into();
        let tree = self.db.open_tree(format!(
            "encrypted_outputs/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        let range_start = range.start.to_le_bytes();
        let range_end = range.end.to_le_bytes();
        let encrypted_outputs: Vec<_> = tree
            .range(range_start..range_end)
            .values()
            .flatten()
            .map(|v| v.to_vec())
            .collect();
        Ok(encrypted_outputs)
    }

    #[tracing::instrument(skip(self))]
    fn insert_encrypted_output<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        encrypted_output: &[(u32, Vec<u8>)],
    ) -> crate::Result<()> {
        let key: HistoryStoreKey = key.into();

        let tree = self.db.open_tree(format!(
            "encrypted_outputs/{}/{}",
            key.chain_id(),
            key.address()
        ))?;
        for (k, v) in encrypted_output {
            tree.insert(k.to_le_bytes(), &*v.clone())?;
        }
        Ok(())
    }

    fn get_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let key: HistoryStoreKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_le_bytes(output))
            }
            None => Ok(0u64),
        }
    }

    fn insert_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let bytes = block_number.to_le_bytes();
        let key: HistoryStoreKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => {
                let mut output = [0u8; 8];
                output.copy_from_slice(&v);
                Ok(u64::from_be_bytes(output))
            }
            None => Ok(block_number),
        }
    }
}

impl EventHashStore for SledStore {
    fn store_event(&self, event: &[u8]) -> crate::Result<()> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        tree.insert(hash, &[])?;
        Ok(())
    }

    fn contains_event(&self, event: &[u8]) -> crate::Result<bool> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        let exists = tree.contains_key(hash)?;
        Ok(exists)
    }

    fn delete_event(&self, event: &[u8]) -> crate::Result<()> {
        let tree = self.db.open_tree("event_hashes")?;
        let hash = ethers::utils::keccak256(event);
        tree.remove(hash)?;
        Ok(())
    }
}

/// SledQueueKey is a key for a queue in Sled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SledQueueKey {
    /// Queue Key for EVM based Transaction Queue.
    EvmTx {
        /// EVM Chain Id.
        chain_id: u32,
        /// an optional key for this transaction.
        optional_key: Option<[u8; 64]>,
    },
    /// Queue Key for Substrate based Transaction Queue.
    SubstrateTx {
        /// Substrate Chain Id.
        chain_id: u32,
        /// an optional key for this transaction.
        optional_key: Option<[u8; 64]>,
    },
    /// Queue Key for Bridge Watcher Command Queue.
    BridgeCmd {
        /// Specific Bridge Key.
        bridge_key: BridgeKey,
    },
}

impl SledQueueKey {
    /// Create a new SledQueueKey from an evm chain id.
    pub fn from_evm_chain_id(chain_id: u32) -> Self {
        Self::EvmTx {
            chain_id,
            optional_key: None,
        }
    }

    /// from_evm_with_custom_key returns an EVM specific SledQueueKey.
    pub fn from_evm_with_custom_key(chain_id: u32, key: [u8; 64]) -> Self {
        Self::EvmTx {
            chain_id,
            optional_key: Some(key),
        }
    }

    /// Create a new SledQueueKey from an substrate chain id.
    pub fn from_substrate_chain_id(chain_id: u32) -> Self {
        Self::SubstrateTx {
            chain_id,
            optional_key: None,
        }
    }

    /// from_substrate_with_custom_key returns an Substrate specific SledQueueKey.
    pub fn from_substrate_with_custom_key(
        chain_id: u32,
        key: [u8; 64],
    ) -> Self {
        Self::SubstrateTx {
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
            Self::SubstrateTx {
                chain_id,
                optional_key,
            } => write!(
                f,
                "SubstrateTx({}, {:?})",
                chain_id,
                optional_key.map(hex::encode)
            ),
            Self::BridgeCmd { bridge_key } => {
                write!(f, "BridgeCmd({bridge_key})")
            }
        }
    }
}

impl QueueKey for SledQueueKey {
    fn queue_name(&self) -> String {
        match self {
            Self::EvmTx { chain_id, .. } => format!("evm_tx_{chain_id}"),
            Self::SubstrateTx { chain_id, .. } => {
                format!("substrate_tx_{chain_id}")
            }
            Self::BridgeCmd { bridge_key, .. } => {
                format!("bridge_cmd_{}", bridge_key.chain_id.chain_id())
            }
        }
    }

    fn item_key(&self) -> Option<[u8; 64]> {
        match self {
            Self::EvmTx { optional_key, .. } => *optional_key,
            Self::SubstrateTx { optional_key, .. } => *optional_key,
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
    fn enqueue_item(&self, key: Self::Key, item: T) -> crate::Result<()> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        let item_bytes = serde_json::to_vec(&item)?;
        // we do everything inside a single transaction
        // so everything happens atomically and if anything fails
        // we revert everything back to the old state.
        tree.transaction::<_, _, std::io::Error>(|db| {
            // get the last id of the queue.
            let last_item_idx = match db.get("last_item_idx")? {
                Some(v) => {
                    let mut output = [0u8; 8];
                    output.copy_from_slice(&v);
                    u64::from_be_bytes(output)
                }
                None => 0u64,
            };
            // increment it.
            let next_idx = last_item_idx + 1u64;
            let idx_bytes = next_idx.to_be_bytes();
            // then save it.
            db.insert("last_item_idx", &idx_bytes)?;
            db.insert("key_prefix", "item")?;
            // we create a item key like so
            // tx_key = 4 bytes prefix ("item") + 8 bytes of the index.
            let mut item_key = [0u8; 4 + std::mem::size_of::<u64>()];
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
    fn dequeue_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        // now we create a lazy iterator that will scan
        // over all saved items in the queue
        // with the specific key prefix.
        let prefix = tree.get("key_prefix")?.unwrap_or_else(|| b"item".into());
        let mut queue = tree.scan_prefix(prefix);
        let (key, value) = match queue.next() {
            Some(Ok(v)) => v,
            _ => {
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
    fn peek_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
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
    fn has_item(&self, key: Self::Key) -> crate::Result<bool> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        if let Some(k) = key.item_key() {
            tree.contains_key(&k[..]).map_err(Into::into)
        } else {
            Ok(false)
        }
    }

    #[tracing::instrument(skip_all, fields(key = %key))]
    fn remove_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
        let tree = self.db.open_tree(format!("queue_{}", key.queue_name()))?;
        let inner_key = match key.item_key() {
            Some(k) => k,
            None => return Ok(None),
        };
        match tree.get(&inner_key[..])? {
            Some(k) => {
                let exists = tree.remove(&k)?;
                tree.remove(inner_key)?;
                let item = exists.and_then(|v| serde_json::from_slice(&v).ok());
                tracing::trace!("removed item from the queue..");
                self.db.flush()?;
                Ok(item)
            }
            None => {
                // not found!
                tracing::trace!("item with key {} not found in queue", key);
                Ok(None)
            }
        }
    }
}

impl ProposalStore for SledStore {
    type Proposal = ();

    #[tracing::instrument(skip_all)]
    fn insert_proposal(&self, proposal: Self::Proposal) -> crate::Result<()> {
        let tree = self.db.open_tree("proposal_store")?;
        tree.insert("TODO", serde_json::to_vec(&proposal)?.as_slice())?;
        Ok(())
    }

    #[tracing::instrument(
        skip_all,
        fields(data_hash = %hex::encode(data_hash))
    )]
    fn remove_proposal(
        &self,
        data_hash: &[u8],
    ) -> crate::Result<Option<Self::Proposal>> {
        let tree = self.db.open_tree("proposal_store")?;
        match tree.get(data_hash)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => {
                tracing::warn!(
                    "Proposal not seen yet; not found in the proposal storage."
                );
                Ok(None)
            }
        }
    }
}

impl<T> TokenPriceCacheStore<T> for SledStore
where
    T: Serialize + DeserializeOwned,
{
    fn get_price(&self, token: &str) -> crate::Result<Option<T>> {
        let tree = self.db.open_tree("token_prices")?;
        match tree.get(token)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn insert_price(&self, token: &str, value: T) -> crate::Result<()> {
        let v = serde_json::to_vec(&value)?;
        let tree = self.db.open_tree("token_prices")?;
        tree.insert(token, v.as_slice())?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
    use webb::evm::ethers::types;
    use webb::evm::ethers::types::transaction::request::TransactionRequest;
    use webb::evm::{
        contract::protocol_solidity::v_anchor_contract::NewNullifierFilter,
        ethers::types::U64,
    };
    use webb_proposals::{TargetSystem, TypedChainId};

    impl SledQueueKey {
        pub fn from_evm_tx(chain_id: u32, tx: &TypedTransaction) -> Self {
            let key = {
                let mut bytes = [0u8; 64];
                let tx_hash =
                    tx.clone().set_chain_id(U64::from(chain_id)).sighash();
                bytes[..32].copy_from_slice(tx_hash.as_fixed_bytes());
                bytes
            };
            Self::from_evm_with_custom_key(chain_id, key)
        }
    }

    #[test]
    fn get_last_block_number_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = 1u32;
        let contract =
            types::H160::from_slice("11111111111111111111".as_bytes());
        let block_number = 20u64;
        let default_block_number = 1u64;
        let history_store_key = (
            TypedChainId::Evm(chain_id),
            TargetSystem::new_contract_address(contract.to_fixed_bytes()),
        );
        store
            .set_last_block_number(history_store_key, block_number)
            .unwrap();

        let block = store
            .get_last_block_number(history_store_key, default_block_number);

        match block {
            Ok(b) => {
                println!("retrieved block {block:?}");
                assert_eq!(b, 20u64);
            }
            Err(e) => {
                println!("Error encountered {e:?}");
            }
        }
    }

    #[test]
    fn get_leaves_with_range_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = 1u32;
        let contract =
            types::H160::from_slice("11111111111111111111".as_bytes());
        let history_store_key = (
            TypedChainId::Evm(chain_id),
            TargetSystem::new_contract_address(contract.to_fixed_bytes()),
        );
        let generated_leaves = (0..20u32)
            .map(|i| (i, types::H256::random().to_fixed_bytes().to_vec()))
            .collect::<Vec<_>>();
        store
            .insert_leaves(history_store_key, &generated_leaves)
            .unwrap();
        let leaves = store
            .get_leaves_with_range(history_store_key, 5..10)
            .unwrap();
        assert_eq!(leaves.len(), 5);
        assert_eq!(
            leaves
                .values()
                .map(|v| v.to_fixed_bytes().to_vec())
                .collect::<Vec<_>>(),
            generated_leaves
                .into_iter()
                .skip(5)
                .take(5)
                .map(|(_, v)| v)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn tx_queue_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = 1u32;
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
            .map(|_| NewNullifierFilter {
                nullifier: types::H256::random().to_fixed_bytes().into(),
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
