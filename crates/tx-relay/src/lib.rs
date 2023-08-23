/// EVM Transactional Relayer.
#[cfg(feature = "evm")]
pub mod evm;

/// Type alias for transaction item key.
pub type TransactionItemKey = ethereum_types::H512;
