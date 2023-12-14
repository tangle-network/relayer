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
