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
//! # Relayer Store Module 🕸️
//!
//! A module for managing the storage of the relayer.
//!
//! ## Overview
//!
//! The relayer store module stores the history of events. Manages the setting
//! and retrieving operations of events.
//!
use std::fmt::{Debug, Display};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use webb::evm::ethers::types;
/// A module for managing in-memory storage of the relayer.
pub mod mem;
/// A module for setting up and managing a [Sled](https://sled.rs)-based database.
pub mod sled;
/// HistoryStoreKey contains the keys used to store the history of events.
#[derive(Eq, PartialEq, Hash)]
pub enum HistoryStoreKey {
    Evm {
        chain_id: types::U256,
        address: types::H160,
    },
    Substrate {
        chain_id: types::U256,
        tree_id: String,
    },
}

/// A Bridge Key is a unique key used for Sending and Receiving Commands to the Signature Bridge
/// It is a combination of the Chain ID and the target system of the Bridge system.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct BridgeKey {
    pub target_system: webb_proposals::TargetSystem,
    pub chain_id: webb_proposals::TypedChainId,
}
/// A way to convert an arbitrary type to a TargetSystem.
pub trait IntoTargetSystem {
    fn into_target_system(self) -> webb_proposals::TargetSystem;
}

/// A way to convert an arbitrary type to a TypedChainId.
pub trait IntoTypedChainId {
    fn into_typed_chain_id(self) -> webb_proposals::TypedChainId;
}

impl IntoTargetSystem for webb_proposals::TargetSystem {
    fn into_target_system(self) -> webb_proposals::TargetSystem {
        self
    }
}

impl IntoTargetSystem for types::Address {
    fn into_target_system(self) -> webb_proposals::TargetSystem {
        webb_proposals::TargetSystem::new_contract_address(self)
    }
}

impl IntoTargetSystem for u32 {
    fn into_target_system(self) -> webb_proposals::TargetSystem {
        webb_proposals::TargetSystem::new_tree_id(self)
    }
}

impl IntoTypedChainId for webb_proposals::TypedChainId {
    fn into_typed_chain_id(self) -> webb_proposals::TypedChainId {
        self
    }
}

impl IntoTypedChainId for types::U256 {
    fn into_typed_chain_id(self) -> webb_proposals::TypedChainId {
        webb_proposals::TypedChainId::Evm(self.as_u32())
    }
}

impl IntoTypedChainId for u32 {
    fn into_typed_chain_id(self) -> webb_proposals::TypedChainId {
        webb_proposals::TypedChainId::Substrate(self)
    }
}

impl BridgeKey {
    pub fn new<TargetSystem, ChainId>(
        target_system: TargetSystem,
        chain_id: ChainId,
    ) -> Self
    where
        TargetSystem: IntoTargetSystem,
        ChainId: IntoTypedChainId,
    {
        Self {
            target_system: target_system.into_target_system(),
            chain_id: chain_id.into_typed_chain_id(),
        }
    }
}

impl HistoryStoreKey {
    /// Returns the chain id of the chain this key is for.
    pub fn chain_id(&self) -> types::U256 {
        match self {
            HistoryStoreKey::Evm { chain_id, .. } => *chain_id,
            HistoryStoreKey::Substrate { chain_id, .. } => *chain_id,
        }
    }
    /// Returns the address of the chain this key is for.
    pub fn address(&self) -> types::H160 {
        match self {
            HistoryStoreKey::Evm { address, .. } => *address,
            HistoryStoreKey::Substrate { tree_id, .. } => {
                // a bit hacky, but we don't have a way to get the address from the tree id
                // so we just pretend it's the address of the node
                let mut address_bytes = vec![];
                address_bytes.extend_from_slice(tree_id.as_bytes());
                address_bytes.resize(20, 0);
                types::H160::from_slice(&address_bytes)
            }
        }
    }

    /// Returns the bytes of the key.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = vec![];
        match self {
            Self::Evm { chain_id, address } => {
                vec.extend_from_slice(&chain_id.as_u128().to_le_bytes());
                vec.extend_from_slice(address.as_bytes());
            }
            Self::Substrate { chain_id, tree_id } => {
                vec.extend_from_slice(&chain_id.as_u128().to_le_bytes());
                vec.extend_from_slice(tree_id.as_bytes());
            }
        }
        vec
    }
}

impl Display for HistoryStoreKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evm { chain_id, address } => {
                write!(f, "Evm({}, {})", chain_id, address)
            }
            Self::Substrate { chain_id, tree_id } => {
                write!(f, "Substrate({}, {})", chain_id, tree_id)
            }
        }
    }
}

impl Display for BridgeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bridge({:?}, {:?})", self.chain_id, self.target_system)
    }
}

impl From<(types::U256, types::Address)> for HistoryStoreKey {
    fn from((chain_id, address): (types::U256, types::Address)) -> Self {
        Self::Evm { chain_id, address }
    }
}

impl From<(types::Address, types::U256)> for HistoryStoreKey {
    fn from((address, chain_id): (types::Address, types::U256)) -> Self {
        Self::Evm { chain_id, address }
    }
}

impl From<(types::U256, String)> for HistoryStoreKey {
    fn from((chain_id, tree_id): (types::U256, String)) -> Self {
        Self::Substrate { chain_id, tree_id }
    }
}

impl From<(String, types::U256)> for HistoryStoreKey {
    fn from((tree_id, chain_id): (String, types::U256)) -> Self {
        Self::Substrate { chain_id, tree_id }
    }
}

/// HistoryStore is a simple trait for storing and retrieving history
/// of block numbers.
pub trait HistoryStore: Clone + Send + Sync {
    /// Sets the new block number for that contract in the cache and returns the old one.
    fn set_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64>;
    /// Get the last block number for that contract.
    /// if not found, returns the `default_block_number`.
    fn get_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64>;

    /// an easy way to call the `get_last_block_number`
    /// where the default block number is `1`.
    fn get_last_block_number_or_default<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64> {
        self.get_last_block_number(key, types::U64::one())
    }
}

/// A Simple Event Store, that does not store the events, instead it store the hash of the event as the key
/// and the value is just empty bytes.
///
/// This is mainly useful to mark the event as processed.
pub trait EventHashStore: Send + Sync + Clone {
    /// Store the event in the store.
    /// the key is the hash of the event.
    fn store_event(&self, event: &[u8]) -> anyhow::Result<()>;

    /// Check if the event is stored in the store.
    /// the key is the hash of the event.
    fn contains_event(&self, event: &[u8]) -> anyhow::Result<bool>;

    /// Delete the event from the store.
    /// the key is the hash of the event.
    fn delete_event(&self, event: &[u8]) -> anyhow::Result<()>;
}

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore: HistoryStore {
    type Output: IntoIterator<Item = types::H256>;

    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<Self::Output>;

    fn insert_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()>;

    // The last deposit info is sent to the client on leaf request
    // So they can verify when the last transaction was sent to maintain
    // their own state of mixers.
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64>;

    fn insert_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64>;
}

/// A Command sent to the Bridge to execute different actions.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum BridgeCommand {
    ExecuteProposalWithSignature {
        data: Vec<u8>,
        signature: Vec<u8>,
    },
    TransferOwnershipWithSignature {
        public_key: Vec<u8>,
        nonce: u32,
        signature: Vec<u8>,
    },
}

/// A trait for retrieving queue keys
pub trait QueueKey {
    fn queue_name(&self) -> String;
    fn item_key(&self) -> Option<[u8; 64]>;
}

/// A Queue Store is a simple trait that help storing items in a queue.
/// The queue is a FIFO queue, that can be used to store anything that can be serialized.
///
/// There is a simple API to get the items from the queue, from a background task for example.
pub trait QueueStore<Item>
where
    Item: Serialize + DeserializeOwned + Clone,
{
    type Key: QueueKey;
    /// Insert an item into the queue.
    fn enqueue_item(&self, key: Self::Key, item: Item) -> anyhow::Result<()>;
    /// Get an item from the queue, and removes it.
    fn dequeue_item(&self, key: Self::Key) -> anyhow::Result<Option<Item>>;
    /// Get an item from the queue, without removing it.
    fn peek_item(&self, key: Self::Key) -> anyhow::Result<Option<Item>>;
    /// Check if the item is in the queue.
    fn has_item(&self, key: Self::Key) -> anyhow::Result<bool>;
    /// Remove an item from the queue.
    fn remove_item(&self, key: Self::Key) -> anyhow::Result<Option<Item>>;
}

impl<S, T> QueueStore<T> for Arc<S>
where
    S: QueueStore<T>,
    T: Serialize + DeserializeOwned + Clone,
{
    type Key = S::Key;

    fn enqueue_item(&self, key: Self::Key, item: T) -> anyhow::Result<()> {
        S::enqueue_item(self, key, item)
    }

    fn dequeue_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        S::dequeue_item(self, key)
    }

    fn peek_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        S::peek_item(self, key)
    }

    fn has_item(&self, key: Self::Key) -> anyhow::Result<bool> {
        S::has_item(self, key)
    }

    fn remove_item(&self, key: Self::Key) -> anyhow::Result<Option<T>> {
        S::remove_item(self, key)
    }
}
/// ProposalStore is a simple trait for inserting and removing proposals.
pub trait ProposalStore {
    type Proposal: Serialize + DeserializeOwned;
    fn insert_proposal(&self, proposal: Self::Proposal) -> anyhow::Result<()>;
    fn remove_proposal(
        &self,
        data_hash: &[u8],
    ) -> anyhow::Result<Option<Self::Proposal>>;
}
