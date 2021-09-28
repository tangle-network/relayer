use webb::evm::ethers::core::types::transaction;
use webb::evm::ethers::types;

pub mod mem;
pub mod sled;

/// HistoryStore is a simple trait for storing and retrieving history
/// of block numbers.
pub trait HistoryStore: Clone + Send + Sync {
    /// Sets the new block number for that contract in the cache and returns the old one.
    fn set_last_block_number(
        &self,
        contract: types::Address,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64>;
    /// Get the last block number for that contract.
    /// if not found, returns the `default_block_number`.
    fn get_last_block_number(
        &self,
        contract: types::Address,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64>;

    /// an easy way to call the `get_last_block_number`
    /// where the default block number is `1`.
    fn get_last_block_number_or_default(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<types::U64> {
        self.get_last_block_number(contract, types::U64::one())
    }
}

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore: HistoryStore {
    type Output: IntoIterator<Item = types::H256>;

    fn get_leaves(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<Self::Output>;

    fn insert_leaves(
        &self,
        contract: types::Address,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()>;

    // The last deposit info is sent to the client on leaf request
    // So they can verify when the last transaction was sent to maintain
    // their own state of mixers.
    fn get_last_deposit_block_number(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<types::U64>;

    fn insert_last_deposit_block_number(
        &self,
        contract: types::Address,
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
        key: &[u8],
        tx: transaction::eip2718::TypedTransaction,
        chain_id: types::U256,
    ) -> anyhow::Result<()>;
    /// Saves the transactions into the queue.
    ///
    /// the chain id is used to create the transaction hash.
    fn enqueue_tx(
        &self,
        tx: transaction::eip2718::TypedTransaction,
        chain_id: types::U256,
    ) -> anyhow::Result<()> {
        let key = tx.sighash(chain_id.as_u64());
        self.enqueue_tx_with_key(key.as_bytes(), tx, chain_id)
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
    /// Lookup for the transaction in the queue by transaction hash and removes it.
    fn remove_tx(
        &self,
        key: &[u8],
        chain_id: types::U256,
    ) -> anyhow::Result<()>;
}
