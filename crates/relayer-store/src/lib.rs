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
use webb::evm::contract::protocol_solidity::signature_bridge::AdminSetResourceWithSignatureCall;
use webb::evm::ethers::types;
use webb::substrate::tangle_runtime::api::{dkg, dkg_proposal_handler};
use webb_proposals::evm::ResourceIdUpdateProposal;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_utils::Result;
/// A module for managing in-memory storage of the relayer.
pub mod mem;
/// A module for setting up and managing a [Sled](https://sled.rs)-based database.
#[cfg(feature = "sled")]
pub mod sled;

/// A store that uses [`sled`](https://sled.rs) as the backend.
#[cfg(feature = "sled")]
pub use self::sled::SledStore;
/// A store that uses in memory data structures as the backend.
pub use mem::InMemoryStore;

/// HistoryStoreKey contains the keys used to store the history of events.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HistoryStoreKey {
    /// Block Queue Key
    Block { chain_id: u32 },
    /// ResourceId Key
    ResourceId {
        resource_id: webb_proposals::ResourceId,
    },
}

/// A Bridge Key is a unique key used for Sending and Receiving Commands to the Signature Bridge
/// It is a combination of the Chain ID and the target system of the Bridge system.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct BridgeKey {
    /// The Chain ID of this Bridge key.
    pub chain_id: webb_proposals::TypedChainId,
}

/// A way to convert an arbitrary type to a TypedChainId.
pub trait IntoTypedChainId {
    /// Consumes Self and returns a TypedChainId.
    fn into_typed_chain_id(self) -> webb_proposals::TypedChainId;
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
    /// Creates new BridgeKey from a TargetSystem and a ChainId.
    pub fn new<ChainId>(chain_id: ChainId) -> Self
    where
        ChainId: IntoTypedChainId,
    {
        Self {
            chain_id: chain_id.into_typed_chain_id(),
        }
    }
    /// Creates new BridgeKey from ResourceId
    pub fn from_resource_id(resource_id: webb_proposals::ResourceId) -> Self {
        Self {
            chain_id: resource_id.typed_chain_id(),
        }
    }
}

impl HistoryStoreKey {
    /// Returns the chain id of the chain this key is for.

    pub fn chain_id(&self) -> u32 {
        match self {
            HistoryStoreKey::Block { chain_id } => *chain_id,
            HistoryStoreKey::ResourceId { resource_id } => {
                resource_id.typed_chain_id().underlying_chain_id()
            }
        }
    }
    /// Returns the address of the chain this key is for.
    pub fn address(&self) -> types::H160 {
        match self {
            HistoryStoreKey::Block { .. } => types::H160::zero(),
            HistoryStoreKey::ResourceId { resource_id } => {
                // take 20 bytes of target system bytes
                types::H160::from_slice(
                    &resource_id.target_system().to_bytes()[6..],
                )
            }
        }
    }

    /// Returns the bytes of the key.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = vec![];
        match self {
            Self::Block { chain_id } => {
                vec.extend_from_slice(&chain_id.to_be_bytes());
            }
            Self::ResourceId { resource_id } => {
                let bytes = resource_id.to_bytes();
                vec.extend_from_slice(&bytes)
            }
        }
        vec
    }
}

impl Display for HistoryStoreKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block { chain_id } => write!(f, "Block(ChainId {chain_id})"),
            Self::ResourceId { resource_id } => {
                write!(
                    f,
                    "ResourceId(ChainId {})",
                    resource_id.typed_chain_id().underlying_chain_id()
                )
            }
        }
    }
}

impl Display for BridgeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bridge({:?})", self.chain_id)
    }
}

impl From<u32> for HistoryStoreKey {
    fn from(chain_id: u32) -> Self {
        Self::Block { chain_id }
    }
}

impl From<ResourceId> for HistoryStoreKey {
    fn from(resource_id: ResourceId) -> Self {
        Self::ResourceId { resource_id }
    }
}

impl From<(TargetSystem, TypedChainId)> for HistoryStoreKey {
    fn from(
        (target_system, typed_chain_id): (TargetSystem, TypedChainId),
    ) -> Self {
        let resource_id = ResourceId::new(target_system, typed_chain_id);
        Self::ResourceId { resource_id }
    }
}

impl From<(TypedChainId, TargetSystem)> for HistoryStoreKey {
    fn from(
        (typed_chain_id, target_system): (TypedChainId, TargetSystem),
    ) -> Self {
        let resource_id = ResourceId::new(target_system, typed_chain_id);
        Self::ResourceId { resource_id }
    }
}

/// HistoryStore is a simple trait for storing and retrieving history
/// of block numbers.
pub trait HistoryStore: Clone + Send + Sync {
    /// Sets the new block number for that contract in the cache and returns the old one.
    fn set_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64>;
    /// Get the last block number for that contract.
    /// if not found, returns the `default_block_number`.
    fn get_last_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: u64,
    ) -> crate::Result<u64>;

    /// an easy way to call the `get_last_block_number`
    /// where the default block number is `1`.
    fn get_last_block_number_or_default<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        self.get_last_block_number(key, 1u64)
    }

    /// Sets the Target Block number (Usually the latest block number of the target chain)
    /// This used to be able to check if we are fully synced with the target chain or not.
    fn set_target_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        block_number: u64,
    ) -> crate::Result<u64>;
    /// Get the target block number.
    /// if not found, returns the `default_block_number`.
    fn get_target_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        default_block_number: u64,
    ) -> crate::Result<u64>;

    /// an easy way to call the `get_target_block_number`.
    fn get_target_block_number_or_default<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64> {
        self.get_target_block_number(key, 1u64)
    }
}

/// A Simple Event Store, that does not store the events, instead it store the hash of the event as the key
/// and the value is just empty bytes.
///
/// This is mainly useful to mark the event as processed.
pub trait EventHashStore: Send + Sync + Clone {
    /// Store the event in the store.
    /// the key is the hash of the event.
    fn store_event(&self, event: &[u8]) -> crate::Result<()>;

    /// Check if the event is stored in the store.
    /// the key is the hash of the event.
    fn contains_event(&self, event: &[u8]) -> crate::Result<bool>;

    /// Delete the event from the store.
    /// the key is the hash of the event.
    fn delete_event(&self, event: &[u8]) -> crate::Result<()>;
}

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore: HistoryStore {
    /// The Output type which is the leaf.
    type Output: IntoIterator<Item = (u32, types::H256)>;

    /// Clear leaf cache on relayer
    fn clear_leaves_cache<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<()>;

    /// Get all the leaves for the given key.
    fn get_leaves<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output>;
    /// Get the leaves for the given key and with the given range of indices.
    fn get_leaves_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output>;

    /// The last deposit info is sent to the client on leaf request
    /// So they can verify when the last transaction was sent to maintain
    /// their own state of vanchors.
    fn get_last_deposit_block_number<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<u64>;

    /// Insert leaves and last deposit block number for the given key.
    fn insert_leaves_and_last_deposit_block_number<
        K: Into<HistoryStoreKey> + Debug + Clone,
    >(
        &self,
        key: K,
        leaves: &[(u32, Vec<u8>)],
        block_number: u64,
    ) -> crate::Result<()>;
}

/// An Encrypted Output Cache Store is a simple trait that would help in
/// getting the encrypted output and insert them with a simple API.
pub trait EncryptedOutputCacheStore: HistoryStore {
    /// The Output type which is the encrypted output.
    type Output: IntoIterator<Item = Vec<u8>>;

    /// Get all the encrypted output for the given key.
    fn get_encrypted_output<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
    ) -> crate::Result<Self::Output>;

    /// Get the encrypted output for the given key and with the given range of indices.
    fn get_encrypted_output_with_range<K: Into<HistoryStoreKey> + Debug>(
        &self,
        key: K,
        range: core::ops::Range<u32>,
    ) -> crate::Result<Self::Output>;

    /// The last deposit info is sent to the client on encrypted_output request
    /// So they can verify when the last transaction was sent to maintain
    /// their own state of vanchors.
    fn get_last_deposit_block_number_for_encrypted_output<
        K: Into<HistoryStoreKey> + Debug,
    >(
        &self,
        key: K,
    ) -> crate::Result<u64>;

    /// Insert encrypted output and last deposit block number for the given key.
    fn insert_encrypted_output_and_last_deposit_block_number<
        K: Into<HistoryStoreKey> + Debug + Clone,
    >(
        &self,
        key: K,
        encrypted_output: &[(u32, Vec<u8>)],
        block_number: u64,
    ) -> crate::Result<()>;
}

/// A Command sent to the Bridge to execute different actions.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum BridgeCommand {
    /// A Command sent to the Signature Bridge to execute a proposal with signature
    ExecuteProposalWithSignature {
        /// The proposal to execute (encoded as bytes)
        data: Vec<u8>,
        /// The signature of the hash of the proposal bytes, Signed by the proposal signing
        /// backend.
        signature: Vec<u8>,
    },
    /// A Command sent to the Signature Bridge to transfer the ownership with a signature.
    TransferOwnershipWithSignature {
        /// The new owner public key.
        public_key: Vec<u8>,
        /// The nonce of this transfer.
        nonce: u32,
        /// The signature of the hash of the nonce+public key, Signed by the proposal signing
        /// backend.
        signature: Vec<u8>,
    },
    /// A Command sent to the Signature Bridge to Add a new Resource with signature.
    /// Sets a new resource for handler contracts that use the IExecutor interface.
    AdminSetResourceWithSignature {
        /// Target resource ID of the proposal header.
        resource_id: [u8; 32],
        /// Secondary resourceID begin mapped to a handler address.
        new_resource_id: [u8; 32],
        /// Address of handler resource will be set for.
        handler_address: [u8; 20],
        /// The nonce of this proposal.
        nonce: u32,
        /// The signature from the governor of the encoded set resource proposal.
        signature: Vec<u8>,
    },
}

impl From<dkg::events::PublicKeySignatureChanged> for BridgeCommand {
    fn from(event: dkg::events::PublicKeySignatureChanged) -> Self {
        Self::TransferOwnershipWithSignature {
            public_key: event.uncompressed_pub_key,
            nonce: event.nonce,
            signature: event.pub_key_sig,
        }
    }
}

impl TryFrom<dkg_proposal_handler::events::ProposalSigned> for BridgeCommand {
    type Error = webb_relayer_utils::Error;
    fn try_from(
        event: dkg_proposal_handler::events::ProposalSigned,
    ) -> Result<Self> {
        Self::try_from((event.data, event.signature))
    }
}

impl TryFrom<(Vec<u8>, Vec<u8>)> for BridgeCommand {
    type Error = webb_relayer_utils::Error;
    fn try_from((data, signature): (Vec<u8>, Vec<u8>)) -> Result<Self> {
        use webb::evm::ethers::prelude::*;
        let data_header_bytes = data
            .get(0..40)
            .ok_or(webb_relayer_utils::Error::InvalidProposalBytes)?;
        let mut header_bytes = [0u8; 40];
        header_bytes.copy_from_slice(data_header_bytes);

        let header = webb_proposals::ProposalHeader::from(header_bytes);
        let function_sig = header.function_signature().into_bytes();
        // checks whether the proposal is a resource id update by using
        // the function signature.
        if function_sig == AdminSetResourceWithSignatureCall::selector() {
            if data.len() != ResourceIdUpdateProposal::LENGTH {
                return Err(webb_relayer_utils::Error::InvalidProposalBytes);
            }
            // Decode the proposal data
            let mut proposal_bytes = [0u8; ResourceIdUpdateProposal::LENGTH];
            proposal_bytes.copy_from_slice(&data[..]);
            let proposal = ResourceIdUpdateProposal::from(proposal_bytes);

            Ok(BridgeCommand::AdminSetResourceWithSignature {
                resource_id: header.resource_id().into_bytes(),
                new_resource_id: proposal.new_resource_id().into_bytes(),
                handler_address: proposal.handler_address(),
                nonce: header.nonce().to_u32(),
                signature,
            })
        } else {
            Ok(BridgeCommand::ExecuteProposalWithSignature { data, signature })
        }
    }
}

/// A trait for retrieving queue keys
pub trait QueueKey {
    /// The Queue name, used as a prefix for the keys.
    fn queue_name(&self) -> String;
    /// an _optional_ different key for the same value stored in the queue.
    ///
    /// This useful for the case when you want to have a direct access to a specific key in the queue.
    /// For example, if you want to remove an item from the queue, you can use this key to directly
    /// remove it from the queue.
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
    /// The type of the queue key.
    type Key: QueueKey;
    /// Insert an item into the queue.
    fn enqueue_item(&self, key: Self::Key, item: Item) -> crate::Result<()>;
    /// Get an item from the queue, and removes it.
    fn dequeue_item(&self, key: Self::Key) -> crate::Result<Option<Item>>;
    /// Get an item from the queue, without removing it.
    fn peek_item(&self, key: Self::Key) -> crate::Result<Option<Item>>;
    /// Check if the item is in the queue.
    fn has_item(&self, key: Self::Key) -> crate::Result<bool>;
    /// Remove an item from the queue.
    fn remove_item(&self, key: Self::Key) -> crate::Result<Option<Item>>;
}

impl<S, T> QueueStore<T> for Arc<S>
where
    S: QueueStore<T>,
    T: Serialize + DeserializeOwned + Clone,
{
    type Key = S::Key;

    fn enqueue_item(&self, key: Self::Key, item: T) -> crate::Result<()> {
        S::enqueue_item(self, key, item)
    }

    fn dequeue_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
        S::dequeue_item(self, key)
    }

    fn peek_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
        S::peek_item(self, key)
    }

    fn has_item(&self, key: Self::Key) -> crate::Result<bool> {
        S::has_item(self, key)
    }

    fn remove_item(&self, key: Self::Key) -> crate::Result<Option<T>> {
        S::remove_item(self, key)
    }
}

/// A trait for Cached Token Price.
pub trait TokenPriceCacheStore<CachedTokenPrice>
where
    CachedTokenPrice: Serialize + DeserializeOwned,
{
    /// Get the cached token price for the given token key.
    /// If the token is not found, it will return `None`.
    fn get_price(
        &self,
        token_key: &str,
    ) -> crate::Result<Option<CachedTokenPrice>>;
    /// Insert the cached token price for the given token key.
    ///
    /// **Note**: this will override the previous value.
    fn insert_price(
        &self,
        token_key: &str,
        value: CachedTokenPrice,
    ) -> crate::Result<()>;
}
