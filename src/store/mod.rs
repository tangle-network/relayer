use std::fmt::{Debug, Display};

use webb::evm::ethers::core::types::transaction;
use webb::evm::ethers::types;

use crate::events_watcher::ProposalEntity;

pub mod mem;
pub mod sled;

#[derive(Eq, PartialEq, Hash)]
pub struct ContractKey {
    pub chain_id: types::U256,
    pub address: types::H160,
}

impl ContractKey {
    pub fn new(chain_id: types::U256, address: types::Address) -> Self {
        Self { chain_id, address }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut vec = vec![];
        vec.extend_from_slice(&self.chain_id.as_u128().to_le_bytes());
        vec.extend_from_slice(self.address.as_bytes());
        vec
    }
}

impl Display for ContractKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "chain_id {}, address {}", self.chain_id, self.address)
    }
}

impl From<(types::U256, types::Address)> for ContractKey {
    fn from((chain_id, address): (types::U256, types::Address)) -> Self {
        Self::new(chain_id, address)
    }
}

impl From<(types::Address, types::U256)> for ContractKey {
    fn from((address, chain_id): (types::Address, types::U256)) -> Self {
        Self::new(chain_id, address)
    }
}

/// HistoryStore is a simple trait for storing and retrieving history
/// of block numbers.
pub trait HistoryStore: Clone + Send + Sync {
    /// Sets the new block number for that contract in the cache and returns the old one.
    fn set_last_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64>;
    /// Get the last block number for that contract.
    /// if not found, returns the `default_block_number`.
    fn get_last_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64>;

    /// an easy way to call the `get_last_block_number`
    /// where the default block number is `1`.
    fn get_last_block_number_or_default<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64> {
        self.get_last_block_number(key, types::U64::one())
    }
}

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore: HistoryStore {
    type Output: IntoIterator<Item = types::H256>;

    fn get_leaves<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<Self::Output>;

    fn insert_leaves<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()>;

    // The last deposit info is sent to the client on leaf request
    // So they can verify when the last transaction was sent to maintain
    // their own state of mixers.
    fn get_last_deposit_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64>;

    fn insert_last_deposit_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64>;
}

/// A Transaction Queue Store is used to store transactions that
/// can be polled from a background task that is only responsible for
/// signing the transactions and send it to the network.
pub trait TxQueueStore {
    /// Saves the transactions into the queue.
    fn enqueue_tx_with_key(
        &self,
        chain_id: types::U256,
        tx: transaction::eip2718::TypedTransaction,
        key: &[u8],
    ) -> anyhow::Result<()>;
    /// Saves the transactions into the queue.
    ///
    /// the chain id is used to create the transaction hash.
    fn enqueue_tx(
        &self,
        chain_id: types::U256,
        tx: transaction::eip2718::TypedTransaction,
    ) -> anyhow::Result<()> {
        let key = tx.sighash(chain_id.as_u64());
        self.enqueue_tx_with_key(chain_id, tx, key.as_bytes())
    }
    /// Polls a transaction from the queue.
    fn dequeue_tx(
        &self,
        chain_id: types::U256,
    ) -> anyhow::Result<Option<transaction::eip2718::TypedTransaction>>;
    /// Reads a transaction from the queue, without actually removing it.
    fn peek_tx(
        &self,
        chain_id: types::U256,
    ) -> anyhow::Result<Option<transaction::eip2718::TypedTransaction>>;
    /// Returns true if the tx is already in the queue.
    fn has_tx(&self, chain_id: types::U256, key: &[u8])
        -> anyhow::Result<bool>;
    /// Lookup for the transaction in the queue by transaction hash and removes it.
    fn remove_tx(
        &self,
        chain_id: types::U256,
        key: &[u8],
    ) -> anyhow::Result<()>;
}

pub trait ProposalStore {
    fn insert_proposal(&self, proposal: ProposalEntity) -> anyhow::Result<()>;
    fn remove_proposal(
        &self,
        data_hash: &[u8],
    ) -> anyhow::Result<Option<ProposalEntity>>;
}
