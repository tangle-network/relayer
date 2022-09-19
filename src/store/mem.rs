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
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use parking_lot::RwLock;
use webb::evm::ethers::types;

use super::{
    EncryptedOutputCacheStore, HistoryStore, HistoryStoreKey, LeafCacheStore,
};

type MemStore = HashMap<HistoryStoreKey, Vec<(u32, types::H256)>>;
type MemStoreForVec = HashMap<HistoryStoreKey, Vec<(u32, Vec<u8>)>>;
/// InMemoryStore is a store that stores the history of events in memory.
#[derive(Clone, Default)]
pub struct InMemoryStore {
    store: Arc<RwLock<MemStore>>,
    store_for_vec: Arc<RwLock<MemStoreForVec>>,
    last_block_numbers: Arc<RwLock<HashMap<HistoryStoreKey, types::U64>>>,
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
        default_block_number: types::U64,
    ) -> crate::Result<types::U64> {
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
        block_number: types::U64,
    ) -> crate::Result<types::U64> {
        let mut guard = self.last_block_numbers.write();
        let val = guard.entry(key.into()).or_insert(block_number);
        let old = *val;
        *val = block_number;
        Ok(old)
    }
}

impl LeafCacheStore for InMemoryStore {
    type Output = Vec<types::H256>;

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output> {
        let guard = self.store.read();
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.1)
            .collect();
        Ok(val)
    }

    #[tracing::instrument(skip(self))]
    fn insert_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, types::H256)],
    ) -> crate::Result<()> {
        let mut guard = self.store.write();
        guard
            .entry(key.into())
            .and_modify(|v| v.extend_from_slice(leaves))
            .or_insert_with(|| leaves.to_vec());
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<types::U64> {
        Ok(types::U64::from(0))
    }

    #[tracing::instrument(skip(self))]
    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> crate::Result<types::U64> {
        Ok(types::U64::from(0))
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
        let val = guard
            .get(&key.into())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.1)
            .collect();
        Ok(val)
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
            .and_modify(|v| v.extend_from_slice(encrypted_outputs))
            .or_insert_with(|| encrypted_outputs.to_vec());
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<types::U64> {
        Ok(types::U64::from(0))
    }

    #[tracing::instrument(skip(self))]
    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> crate::Result<types::U64> {
        Ok(types::U64::from(0))
    }
}
