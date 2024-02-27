// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

mod evm_tx_queue;
use std::sync::Arc;

use ethereum_types::U256;
#[doc(hidden)]
pub use evm_tx_queue::*;

use url::Url;
use webb::evm::ethers::{providers::Middleware, signers::LocalWallet};
use webb_relayer_utils::Result;

/// Config trait for EVM tx queue.
#[async_trait::async_trait]
pub trait EvmTxQueueConfig {
    type EtherClient: Middleware;
    /// Maximum number of milliseconds to wait before dequeuing a transaction from
    /// the queue.
    fn max_sleep_interval(&self, chain_id: &U256) -> Result<u64>;
    /// Block confirmations
    fn block_confirmations(&self, chain_id: &U256) -> Result<u8>;
    /// Block Explorer for this chain.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    fn explorer(&self, chain_id: &U256) -> Result<Option<Url>>;
    /// Returns a new `EthereumProvider`.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain id.
    async fn get_evm_provider(
        &self,
        chain_id: &U256,
    ) -> Result<Arc<Self::EtherClient>>;
    /// Returns an EVM wallet.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain id.
    async fn get_evm_wallet(&self, chain_id: &U256) -> Result<LocalWallet>;
}
