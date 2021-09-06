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
}
