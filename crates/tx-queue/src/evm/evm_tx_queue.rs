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

use std::sync::Arc;
use std::time::Duration;

use ethereum_types::{H256, U64};
use futures::TryFutureExt;
use rand::Rng;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::middleware::SignerMiddleware;
use webb::evm::ethers::prelude::TimeLag;
use webb::evm::ethers::providers::Middleware;

use webb::evm::ethers::types;
use webb_relayer_context::RelayerContext;
use webb_relayer_store::queue::{
    QueueItem, QueueItemState, QueueStore, TransactionQueueItemKey,
};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_utils::clickable_link::ClickableLink;

/// The TxQueue stores transaction requests so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct TxQueue<S: QueueStore<TypedTransaction>> {
    ctx: RelayerContext,
    chain_id: types::U256,
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
    /// * `chain_id` - The chainId that this queue is for
    /// * `store` - [Sled](https://sled.rs)-based database store
    pub fn new(
        ctx: RelayerContext,
        chain_id: types::U256,
        store: Arc<S>,
    ) -> Self {
        Self {
            ctx,
            chain_id,
            store,
        }
    }
    /// Starts the TxQueue service.
    ///
    /// Returns a future that resolves `Ok(())` on success, otherwise returns an error.
    #[tracing::instrument(skip_all, fields(chain = %self.chain_id))]
    pub async fn run(self) -> webb_relayer_utils::Result<()> {
        let provider = self.ctx.evm_provider(&self.chain_id).await?;
        let wallet = self.ctx.evm_wallet(self.chain_id).await?;
        let signer_client = SignerMiddleware::new(provider, wallet);

        let chain_config = self
            .ctx
            .config
            .evm
            .get(&self.chain_id.as_u64().to_string())
            .ok_or_else(|| webb_relayer_utils::Error::ChainNotFound {
                chain_id: self.chain_id.to_string(),
            })?;

        // TimeLag client
        let client =
            TimeLag::new(signer_client, chain_config.block_confirmations);
        let chain_id = client
            .get_chainid()
            .map_err(|_| {
                webb_relayer_utils::Error::Generic(
                    "Failed to fetch chain id from client",
                )
            })
            .await?
            .as_u32();

        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::TxQueue,
            ty = "EVM",
            chain_id = %chain_id,
            starting = true,
        );
        let metrics_clone = self.ctx.metrics.clone();
        let task = || async {
            loop {
                let maybe_item = store
                    .peek_item(SledQueueKey::from_evm_chain_id(chain_id))?;
                let maybe_explorer = &chain_config.explorer;
                let mut tx_hash: H256;
                if let Some(item) = maybe_item {
                    let mut raw_tx = item.clone().inner();
                    raw_tx.set_chain_id(U64::from(chain_id));
                    let my_tx_hash = raw_tx.sighash();
                    tx_hash = my_tx_hash;
                    tracing::debug!(?tx_hash, tx = ?raw_tx, "Found tx in queue");

                    let tx_item_key =
                        item.clone().inner().item_key(my_tx_hash.into());

                    // Remove tx item from queue if expired.
                    if item.is_expired() {
                        tracing::warn!(
                            ?tx_hash,
                            "Tx is expired, removing it from queue"
                        );
                        store.remove_item(
                            SledQueueKey::from_evm_with_custom_key(
                                chain_id,
                                tx_item_key,
                            ),
                        )?;
                        continue;
                    }

                    // Process transactions only when in pending state.
                    if item.state() != QueueItemState::Pending {
                        tracing::debug!(
                            ?tx_hash,
                            item_state = ?item.state(),
                            "Tx is not in pending state, skipping"
                        );
                        continue;
                    }
                    // update transaction status as Processing.
                    store.update_item(
                        SledQueueKey::from_evm_with_custom_key(
                            chain_id,
                            tx_item_key,
                        ),
                        |item: &mut QueueItem<TypedTransaction>| {
                            let state = QueueItemState::Processing {
                                step: "Item picked, processing".to_string(),
                                progress: Some(0.0),
                            };
                            item.set_state(state);
                            Ok(())
                        },
                    )?;
                    // dry run test
                    let dry_run_outcome =
                        client.call(&raw_tx.clone(), None).await;
                    match dry_run_outcome {
                        Ok(_) => {
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                dry_run = "passed",
                                %tx_hash,
                            );
                            // update transaction status as Processing and set progress.
                            store.update_item(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Processing {
                                        step: "Dry run passed".to_string(),
                                        progress: Some(0.5),
                                    };
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;
                        }
                        Err(err) => {
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                errored = true,
                                error = %err,
                                dry_run = "failed",
                                %tx_hash,
                            );
                            // update transaction status as Failed and re insert into queue.
                            store.shift_item_to_end(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Failed {
                                        reason: err.to_string(),
                                    };
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;

                            continue; // keep going.
                        }
                    }

                    let pending_tx =
                        client.send_transaction(raw_tx.clone(), None);
                    let tx = match pending_tx.await {
                        Ok(pending) => {
                            tx_hash = *pending;
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                pending = true,
                                %tx_hash,
                            );

                            let tx_hash_string = format!("0x{tx_hash:x}");
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{tx_hash_string}"));
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
                            // update transaction progress.
                            store.update_item(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Processing {
                                        step:
                                            "Transaction submitted on chain.."
                                                .to_string(),
                                        progress: Some(0.8),
                                    };
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;
                            pending.interval(Duration::from_millis(1000)).await
                        }
                        Err(e) => {
                            let tx_hash_string = format!("0x{tx_hash:x}");
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{tx_hash_string}"));
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
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                errored = true,
                                %tx_hash,
                                error = %e,
                            );

                            // update transaction status as Failed
                            store.shift_item_to_end(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Failed {
                                        reason: e.to_string(),
                                    };
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;

                            continue; // keep going.
                        }
                    };
                    match tx {
                        Ok(Some(receipt)) => {
                            let tx_hash_string =
                                format!("0x{:x}", receipt.transaction_hash);
                            match receipt.status {
                                Some(v) if v.is_zero() => {
                                    tracing::info!(
                                        "Tx {} Failed",
                                        tx_hash_string,
                                    );
                                    continue;
                                }
                                _ => {}
                            }

                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{tx_hash_string}"));
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
                            let gas_price =
                                receipt.gas_used.unwrap_or_default();
                            // metrics for  transaction processed by evm tx queue
                            let metrics = metrics_clone.lock().await;
                            metrics.proposals_processed_tx_queue.inc();
                            metrics.proposals_processed_evm_tx_queue.inc();
                            // gas spent metric
                            metrics.gas_spent.inc_by(gas_price.as_u64() as f64);
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                finalized = true,
                                %tx_hash,
                            );

                            // update transaction progress as processed
                            store.shift_item_to_end(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Processed;
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;
                        }
                        Ok(None) => {
                            // this should never happen
                            // as we already know that is a bug in ethers
                            // about timeing, so we already wait a bit
                            // and increased the time interval for checking for
                            // transaction status.
                            let tx_hash_string = format!("0x{tx_hash:x}");
                            tracing::warn!(
                                "Tx {} Dropped from Mempool!!",
                                tx_hash_string
                            );
                            // Re insert transaction in the queue.
                            store.shift_item_to_end(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Pending;
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;
                        }
                        Err(e) => {
                            let reason = e.to_string();
                            let tx_hash_string = format!("0x{tx_hash:x}");
                            if let Some(mut url) = maybe_explorer.clone() {
                                url.set_path(&format!("tx/{tx_hash_string}"));
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
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "EVM",
                                chain_id = %chain_id,
                                errored = true,
                                %tx_hash,
                                error = %e,
                            );
                            // Update transaction status and re insert in the queue.
                            store.shift_item_to_end(
                                SledQueueKey::from_evm_with_custom_key(
                                    chain_id,
                                    tx_item_key,
                                ),
                                |item: &mut QueueItem<TypedTransaction>| {
                                    let state = QueueItemState::Failed {
                                        reason: e.to_string(),
                                    };
                                    item.set_state(state);
                                    Ok(())
                                },
                            )?;
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
        // transaction queue backoff metric
        let metrics = self.ctx.metrics.lock().await;
        metrics.transaction_queue_back_off.inc();
        metrics.evm_transaction_queue_back_off.inc();
        drop(metrics);
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
