/// EVM Transactional Relayer.
#[cfg(feature = "evm")]
pub mod evm;
/// Substrate Transactional Relayer.
#[cfg(feature = "substrate")]
pub mod substrate;

/// Maximum refund amount per relay transaction in USD.
const MAX_REFUND_USD: f64 = 5.;
/// Amount of profit that the relay should make with each transaction (in USD).
const TRANSACTION_PROFIT_USD: f64 = 5.;

/// Type alias for transaction item key.
pub type TransactionItemKey = ethereum_types::H512;
