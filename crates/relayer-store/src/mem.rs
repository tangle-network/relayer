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

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::sync::Arc;

use parking_lot::RwLock;
use webb::evm::ethers::types;

use crate::TokenPriceCacheStore;

use super::{
    EncryptedOutputCacheStore, HistoryStore, HistoryStoreKey, LeafCacheStore,
};

type MemStore = HashMap<HistoryStoreKey, Vec<types::H256>>;
type MemStoreForVec = HashMap<HistoryStoreKey, Vec<Vec<u8>>>;
type MemStoreForMap = HashMap<HistoryStoreKey, BTreeMap<u32, types::H256>>;
/// InMemoryStore is a store that stores the history of events in memory.
#[derive(Clone, Default)]
pub struct InMemoryStore {
    _store: Arc<RwLock<MemStore>>,
    store_for_vec: Arc<RwLock<MemStoreForVec>>,
    store_for_map: Arc<RwLock<MemStoreForMap>>,
    last_block_numbers: Arc<RwLock<HashMap<HistoryStoreKey, u64>>>,
    target_block_numbers: Arc<RwLock<HashMap<HistoryStoreKey, u64>>>,
    last_deposit_block_numbers: Arc<RwLock<HashMap<HistoryStoreKey, u64>>>,
    encrypted_output_last_deposit_block_numbers:
        Arc<RwLock<HashMap<HistoryStoreKey, u64>>>,
    token_prices_cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl std::fmt::Debug for InMemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryStore").finish()
    }
}

impl HistoryStore for InMemoryStore {
    #[tracing::instrument(skip(self))]
    fn get_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: u64,
    ) -> crate::Result<u64> {
        let guard = self.last_block_numbers.read();
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or(default_block_number);
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn set_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        let mut guard = self.last_block_numbers.write();
        let val = guard.entry(key.into()).or_insert(block_number);
        let old = *val;
        *val = block_number;
        Ok(old)
    }

    fn get_last_block_number_or_default<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        self.get_last_block_number(key, 1u64)
    }

    #[tracing::instrument(skip(self))]
    fn set_target_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        let mut guard = self.target_block_numbers.write();
        let val = guard.entry(key.into()).or_insert(block_number);
        let old = *val;
        *val = block_number;
        Ok(old)
    }

    #[tracing::instrument(skip(self))]
    fn get_target_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: u64,
    ) -> crate::Result<u64> {
        let guard = self.target_block_numbers.read();
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or(default_block_number);
        Ok(val)
    }
}

impl LeafCacheStore for InMemoryStore {
    type Output = BTreeMap<u32, types::H256>;

    #[tracing::instrument(skip(self))]
    fn clear_leaves_cache<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<()> {
        let mut guard = self.store_for_map.write();
        guard.clear();
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let guard = self.store_for_map.read();
        let val = guard.get(&key.into()).cloned().unwrap_or_default();
        Ok(val)
    }

    fn get_leaves_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output> {
        let guard = self.store_for_map.read();
        let val = guard.get(&key.into()).cloned().unwrap_or_default();
        let iter = val
            .into_iter()
            .skip(range.start as usize)
            .take((range.end - range.start) as usize);
        Ok(iter.collect())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        let guard = self.last_deposit_block_numbers.read();
        let default_block_number = 0u64;
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or(default_block_number);
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn insert_leaves_and_last_deposit_block_number<
        K: Into<HistoryStoreKey> + Debug + Clone,
    >(
        &self,
        key: K,
        leaves: &[(u32, Vec<u8>)],
        block_number: u64,
    ) -> crate::Result<()> {
        let mut guard1 = self.store_for_map.write();
        let mut guard2 = self.last_deposit_block_numbers.write();
        let mut guard3 = self.last_block_numbers.write();
        {
            // 1. Insert leaves
            guard1
                .entry(key.clone().into())
                .and_modify(|v| {
                    for (index, leaf) in leaves {
                        v.insert(*index, types::H256::from_slice(leaf));
                    }
                })
                .or_insert_with(|| {
                    let mut map = BTreeMap::new();
                    for (index, leaf) in leaves {
                        map.insert(*index, types::H256::from_slice(leaf));
                    }
                    map
                });
            // 2. Insert last deposit block number
            guard2.insert(key.clone().into(), block_number);
            // 3. Insert last block number
            guard3.entry(key.into()).or_insert(block_number);
        }
        Ok(())
    }
}

impl EncryptedOutputCacheStore for InMemoryStore {
    type Output = Vec<Vec<u8>>;

    #[tracing::instrument(skip(self))]
    fn get_encrypted_output<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let guard = self.store_for_vec.read();
        let val = guard.get(&key.into()).cloned().unwrap_or_default();
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn get_encrypted_output_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output> {
        let guard = self.store_for_vec.read();
        let val = guard.get(&key.into()).cloned().unwrap_or_default();
        let iter = val
            .into_iter()
            .skip(range.start as usize)
            .take((range.end - range.start) as usize);
        Ok(iter.collect())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        let guard = self.encrypted_output_last_deposit_block_numbers.read();
        let default_block_number = 0u64;
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or(default_block_number);
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn insert_encrypted_output_and_last_deposit_block_number<
        K: Into<HistoryStoreKey> + Debug + Clone,
    >(
        &self,
        key: K,
        encrypted_outputs: &[(u32, Vec<u8>)],
        block_number: u64,
    ) -> crate::Result<()> {
        let mut guard1 = self.store_for_vec.write();
        let mut guard2 = self.last_deposit_block_numbers.write();
        {
            guard1
                .entry(key.clone().into())
                .and_modify(|v| {
                    for (index, encrypted_output) in encrypted_outputs {
                        v.insert(*index as usize, encrypted_output.clone());
                    }
                })
                .or_insert_with(|| {
                    encrypted_outputs.iter().map(|v| v.1.clone()).collect()
                });
            guard2.insert(key.into(), block_number);
        }
        Ok(())
    }
}

impl<T> TokenPriceCacheStore<T> for InMemoryStore
where
    T: serde::Serialize + serde::de::DeserializeOwned + Clone + Debug,
{
    fn get_price(&self, token: &str) -> crate::Result<Option<T>> {
        self.token_prices_cache
            .read()
            .get(token)
            .map(|v| serde_json::from_slice(v))
            .transpose()
            .map_err(Into::into)
    }

    fn insert_price(&self, token: &str, value: T) -> crate::Result<()> {
        let v = serde_json::to_vec(&value)?;
        self.token_prices_cache.write().insert(token.to_string(), v);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_gets_all_leaves() {
        // a simple test to show that we can get all the leaves that we inserted.
        let store = InMemoryStore::default();
        let generated_leaves = (0..20u32)
            .map(|i| (i, types::H256::random().to_fixed_bytes().to_vec()))
            .collect::<Vec<_>>();
        let key = HistoryStoreKey::from(1u32);
        let block_number = 20u64;
        store
            .insert_leaves_and_last_deposit_block_number(
                key,
                &generated_leaves,
                block_number,
            )
            .unwrap();
        let leaves = store.get_leaves(key).unwrap();
        assert_eq!(
            leaves
                .into_iter()
                .map(|v| v.1.to_fixed_bytes().to_vec())
                .collect::<Vec<_>>(),
            generated_leaves
                .iter()
                .map(|v| v.1.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn it_gets_leaves_with_range() {
        // a simple test to show that we can get leaves with range that we inserted.
        let store = InMemoryStore::default();
        let generated_leaves = (0..20u32)
            .map(|i| (i, types::H256::random().to_fixed_bytes().to_vec()))
            .collect::<Vec<_>>();
        let key = HistoryStoreKey::from(1u32);
        let block_number = 20u64;
        store
            .insert_leaves_and_last_deposit_block_number(
                key,
                &generated_leaves,
                block_number,
            )
            .unwrap();
        let leaves = store.get_leaves_with_range(key, 5..10).unwrap();
        assert_eq!(
            leaves
                .into_iter()
                .map(|v| v.1.to_fixed_bytes().to_vec())
                .collect::<Vec<_>>(),
            generated_leaves
                .iter()
                .skip(5)
                .take(5)
                .map(|v| v.1.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn insert_leaves_and_last_deposit_block_number() {
        // a simple test to show that we can get all the leaves that we inserted.
        let store = InMemoryStore::default();
        let generated_leaves = (0..20u32)
            .map(|i| (i, types::H256::random().to_fixed_bytes().to_vec()))
            .collect::<Vec<_>>();
        let key = HistoryStoreKey::from(1u32);
        let block_number = 20u64;
        store
            .insert_leaves_and_last_deposit_block_number(
                key,
                &generated_leaves,
                block_number,
            )
            .unwrap();
        let last_deposit_block_number =
            store.get_last_deposit_block_number(key).unwrap();
        assert_eq!(last_deposit_block_number, block_number);
        let leaves = store.get_leaves(key).unwrap();
        assert!(leaves
            .into_iter()
            .map(|v| v.1.to_fixed_bytes().to_vec())
            .collect::<Vec<_>>()
            .iter()
            .eq(generated_leaves
                .iter()
                .map(|v| v.1.clone())
                .collect::<Vec<_>>()
                .iter()));
    }
}
