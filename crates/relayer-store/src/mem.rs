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

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use parking_lot::RwLock;
use webb::evm::ethers::types;

use super::{
    EncryptedOutputCacheStore, HistoryStore, HistoryStoreKey, LeafCacheStore,
};

type MemStore = HashMap<HistoryStoreKey, Vec<types::H256>>;
type MemStoreForVec = HashMap<HistoryStoreKey, Vec<Vec<u8>>>;
/// InMemoryStore is a store that stores the history of events in memory.
#[derive(Clone, Default)]
pub struct InMemoryStore {
    _store: Arc<RwLock<MemStore>>,
    store_for_vec: Arc<RwLock<MemStoreForVec>>,
    last_block_numbers: Arc<RwLock<HashMap<HistoryStoreKey, u64>>>,
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
}

impl LeafCacheStore for InMemoryStore {
    type Output = Vec<Vec<u8>>;

    #[tracing::instrument(skip(self))]
    fn clear_leaves_cache<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<()> {
        let mut guard = self.store_for_vec.write();
        guard.clear();
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let guard = self.store_for_vec.read();
        let val = guard.get(&key.into()).cloned().unwrap_or_default();
        Ok(val)
    }

    fn get_leaves_with_range<K: Into<HistoryStoreKey> + Debug>(
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
    fn insert_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, Vec<u8>)],
    ) -> crate::Result<()> {
        let mut guard = self.store_for_vec.write();
        guard
            .entry(key.into())
            .and_modify(|v| {
                for (index, leaf) in leaves {
                    v.insert(*index as usize, leaf.clone());
                }
            })
            .or_insert_with(|| leaves.iter().map(|v| v.1.clone()).collect());
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        Ok(0u64)
    }

    #[tracing::instrument(skip(self))]
    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        Ok(0u64)
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
    fn insert_encrypted_output<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        encrypted_outputs: &[(u32, Vec<u8>)],
    ) -> crate::Result<()> {
        let mut guard = self.store_for_vec.write();
        guard
            .entry(key.into())
            .and_modify(|v| {
                for (index, encrypted_output) in encrypted_outputs {
                    v.insert(*index as usize, encrypted_output.clone());
                }
            })
            .or_insert_with(|| {
                encrypted_outputs.iter().map(|v| v.1.clone()).collect()
            });
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        Ok(0u64)
    }

    #[tracing::instrument(skip(self))]
    fn insert_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64> {
        Ok(0u64)
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
        store.insert_leaves(key, &generated_leaves).unwrap();
        let leaves = store.get_leaves(key).unwrap();
        assert_eq!(
            leaves,
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
        store.insert_leaves(key, &generated_leaves).unwrap();
        let leaves = store.get_leaves_with_range(key, 5..10).unwrap();
        assert_eq!(
            leaves,
            generated_leaves
                .iter()
                .skip(5)
                .take(5)
                .map(|v| v.1.clone())
                .collect::<Vec<_>>()
        );
    }
}
