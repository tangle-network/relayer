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
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use ethereum_types::H256;
use futures::TryFutureExt;
use rand::Rng;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::middleware::SignerMiddleware;
use webb::evm::ethers::providers::Middleware;

use crate::context::RelayerContext;
use crate::store::sled::SledQueueKey;
use crate::store::QueueStore;
use crate::utils::ClickableLink;

/// The TxQueue stores transaction requests so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct TxQueue<S: QueueStore<TypedTransaction>> {
    ctx: RelayerContext,
    chain_name: String,
    store: Arc<S>,
}

impl<S> TxQueue<S>
where
    S: QueueStore<TypedTransaction, Key = SledQueueKey>,
{
    /// Creates a new TxQueue instance.
    ///
    /// Returns a TxQueue instance.
    ///
    /// # Arguments
    ///
    /// * `ctx` - RelayContext reference that holds the configuration
    /// * `chain_name` - The name of the chain that this queue is for
    /// * `store` - [Sled](https://sled.rs)-based database store
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::tx_queue::TxQueue;
    /// let tx_queue = TxQueue::new(ctx, chain_name.clone(), store);
    /// ```
    pub fn new(ctx: RelayerContext, chain_name: String, store: Arc<S>) -> Self {
        Self {
            ctx,
            chain_name,
            store,
        }
    }
    /// Starts the TxQueue service.
    ///
    /// Returns a future that resolves `Ok(())` on success, otherwise returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::tx_queue::TxQueue;
    /// let tx_queue = TxQueue::new(ctx, chain_name.clone(), store);
    ///  let task = async move {
    ///     tokio::select! {
    ///         _ = tx_queue.run() => {
    ///             // do something
    ///         },
    ///         _ = shutdown_signal.recv() => {
    ///             // do something
    ///         },
    ///     }
    /// };
    /// ```
    #[tracing::instrument(skip_all, fields(chain = %self.chain_name))]
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let provider = self.ctx.evm_provider(&self.chain_name).await?;
        let wallet = self.ctx.evm_wallet(&self.chain_name).await?;
        let client = Arc::new(SignerMiddleware::new(provider, wallet));
        let chain_config = self
            .ctx
            .config
            .evm
            .get(&self.chain_name)
            .context("Chain not configured")?;
        let chain_id = client.get_chainid().await?;
        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async {
            loop {
                tracing::trace!("Checking for any txs in the queue ...");
                let maybe_tx = store
                    .dequeue_item(SledQueueKey::from_evm_chain_id(chain_id))?;
                let maybe_explorer = &chain_config.explorer;
                let mut tx_hash: H256;
                if let Some(raw_tx) = maybe_tx {
                    let my_tx_hash = raw_tx.sighash(chain_id.as_u64());
                    tx_hash = my_tx_hash;
                    let pending_tx = client
                        .send_transaction(raw_tx.clone(), None)
                        .map_err(anyhow::Error::from);
                    let tx = match pending_tx.await {
                        Ok(pending) => {
                            tx_hash = *pending;
                            let tx_hash_string = format!("0x{:x}", tx_hash);
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{}", tx_hash_string));
                                let clickable_link = ClickableLink::new(
                                    &tx_hash_string,
                                    url.as_str(),
                                );
                                tracing::info!(
                                    "Tx {} is submitted and pending!",
                                    clickable_link,
                                );
                            } else {
                                tracing::info!(
                                    "Tx {} is submitted and pending!",
                                    tx_hash_string,
                                );
                            }
                            let result = pending
                                .interval(Duration::from_millis(1000))
                                .await;
                            result
                        }
                        Err(e) => {
                            let tx_hash_string = format!("0x{:x}", tx_hash);
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{}", tx_hash_string));
                                let clickable_link = ClickableLink::new(
                                    &tx_hash_string,
                                    url.as_str(),
                                );
                                tracing::error!(
                                    "Error while sending tx {}, {}",
                                    clickable_link,
                                    e,
                                );
                            } else {
                                tracing::error!(
                                    "Error while sending tx {}, {}",
                                    tx_hash_string,
                                    e
                                );
                            }
                            continue; // keep going.
                        }
                    };
                    match tx {
                        Ok(Some(receipt)) => {
                            let tx_hash_string =
                                format!("0x{:x}", receipt.transaction_hash);
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{}", tx_hash_string));
                                let clickable_link = ClickableLink::new(
                                    &tx_hash_string,
                                    url.as_str(),
                                );
                                tracing::info!(
                                    "Tx {} Finalized",
                                    clickable_link
                                );
                            } else {
                                tracing::info!(
                                    "Tx {} Finalized",
                                    tx_hash_string,
                                );
                            }
                        }
                        Ok(None) => {
                            // this should never happen
                            // as we already know that is a bug in ethers
                            // about timeing, so we already wait a bit
                            // and increased the time interval for checking for
                            // transaction status.
                            let tx_hash_string = format!("0x{:x}", tx_hash);
                            tracing::warn!(
                                "Tx {} Dropped from Mempool!!",
                                tx_hash_string
                            );
                            // enquing the tx again
                            store.enqueue_item(
                                SledQueueKey::from_evm_chain_id(chain_id),
                                raw_tx,
                            )?;
                        }
                        Err(e) => {
                            let reason = e.to_string();
                            let tx_hash_string = format!("0x{:x}", tx_hash);
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{}", tx_hash_string));
                                let clickable_link = ClickableLink::new(
                                    &tx_hash_string,
                                    url.as_str(),
                                );
                                tracing::error!(
                                    "Tx {} Errored: {}",
                                    clickable_link,
                                    reason,
                                );
                            } else {
                                tracing::error!(
                                    "Tx {} Errored: {}",
                                    tx_hash_string,
                                    reason,
                                );
                            }
                        }
                    };
                }
                // sleep for a random amount of time.
                let max_sleep_interval =
                    chain_config.tx_queue.max_sleep_interval;
                let s =
                    rand::thread_rng().gen_range(1_000..=max_sleep_interval);
                tracing::trace!("next queue round after {} ms", s);
                tokio::time::sleep(Duration::from_millis(s)).await;
            }
        };
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
