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

use std::sync::Arc;
use std::time::Duration;

use ethereum_types::U64;
use futures::TryFutureExt;
use rand::Rng;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::middleware::SignerMiddleware;
use webb::evm::ethers::prelude::TimeLag;
use webb::evm::ethers::providers::Middleware;

use webb::evm::ethers::types;
use webb_relayer_store::queue::{
    QueueItemState, QueueStore, TransactionQueueItemKey,
};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_utils::clickable_link::ClickableLink;

use super::EvmTxQueueConfig;

/// The TxQueue stores transaction requests so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct TxQueue<S, C>
where
    S: QueueStore<TypedTransaction>,
    C: EvmTxQueueConfig,
{
    ctx: C,
    chain_id: types::U256,
    store: Arc<S>,
}

impl<S, C> TxQueue<S, C>
where
    S: QueueStore<TypedTransaction, Key = SledQueueKey>,
    C: EvmTxQueueConfig,
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
    pub fn new(ctx: C, chain_id: types::U256, store: Arc<S>) -> Self {
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
        let provider = self.ctx.get_evm_provider(&self.chain_id).await?;
        let wallet = self.ctx.get_evm_wallet(&self.chain_id).await?;
        let signer_client = SignerMiddleware::new(provider, wallet);
        let block_confirmations =
            self.ctx.block_confirmations(&self.chain_id)?;

        // TimeLag client
        let client = TimeLag::new(signer_client, block_confirmations);
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
        let task = || async {
            loop {
                let maybe_item = store
                    .peek_item(SledQueueKey::from_evm_chain_id(chain_id))?;
                let maybe_explorer = self.ctx.explorer(&self.chain_id)?;
                let Some(item) = maybe_item else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let mut raw_tx = item.clone().inner();
                raw_tx.set_chain_id(U64::from(chain_id));
                let tx_hash = raw_tx.sighash();

                let tx_item_key = item.clone().inner().item_key();

                // Remove tx item from queue if expired.
                if item.is_expired() {
                    tracing::trace!(
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
                    // Shift it back to the end of the queue
                    // so that we can process other items.
                    store.shift_item_to_end(
                        SledQueueKey::from_evm_with_custom_key(
                            chain_id,
                            tx_item_key,
                        ),
                        // Do not update the state.
                        |_| Ok(()),
                    )?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                tracing::info!(?tx_hash, tx = ?raw_tx, "Found tx in queue");
                // update transaction status as Processing.
                store.update_item(
                    SledQueueKey::from_evm_with_custom_key(
                        chain_id,
                        tx_item_key,
                    ),
                    |item| {
                        let state = QueueItemState::Processing {
                            step: "Item picked, processing".to_string(),
                            progress: Some(0.0),
                        };
                        item.set_state(state);
                        Ok(())
                    },
                )?;
                // dry run test
                let dry_run_outcome = client.call(&raw_tx.clone(), None).await;
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
                            |item| {
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
                            |item| {
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

                let pending_tx = client.send_transaction(raw_tx.clone(), None);
                let tx = match pending_tx.await {
                    Ok(pending) => {
                        let signed_tx_hash = *pending;
                        tracing::event!(
                            target: webb_relayer_utils::probe::TARGET,
                            tracing::Level::DEBUG,
                            kind = %webb_relayer_utils::probe::Kind::TxQueue,
                            ty = "EVM",
                            chain_id = %chain_id,
                            pending = true,
                            raw_tx_hash = %tx_hash,
                            %signed_tx_hash,
                        );

                        let tx_hash_string = format!("0x{signed_tx_hash:x}");
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
                            |item| {
                                let state = QueueItemState::Processing {
                                    step: "Transaction submitted on chain.."
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
                            raw_tx_hash = %tx_hash,
                            error = %e,
                        );

                        // update transaction status as Failed
                        store.shift_item_to_end(
                            SledQueueKey::from_evm_with_custom_key(
                                chain_id,
                                tx_item_key,
                            ),
                            |item| {
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
                                tracing::info!("Tx {} Failed", tx_hash_string);
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
                            tracing::info!("Tx {} Finalized", clickable_link);
                        } else {
                            tracing::info!("Tx {} Finalized", tx_hash_string,);
                        }

                        tracing::event!(
                            target: webb_relayer_utils::probe::TARGET,
                            tracing::Level::DEBUG,
                            kind = %webb_relayer_utils::probe::Kind::TxQueue,
                            ty = "EVM",
                            chain_id = %chain_id,
                            finalized = true,
                            raw_tx_hash = %tx_hash,
                            signed_tx_hash = %receipt.transaction_hash,
                        );

                        // update transaction progress as processed
                        store.shift_item_to_end(
                            SledQueueKey::from_evm_with_custom_key(
                                chain_id,
                                tx_item_key,
                            ),
                            |item| {
                                let state = QueueItemState::Processed {
                                    tx_hash: receipt.transaction_hash,
                                };
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
                            |item| {
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
                            |item| {
                                let state = QueueItemState::Failed {
                                    reason: e.to_string(),
                                };
                                item.set_state(state);
                                Ok(())
                            },
                        )?;
                    }
                };

                // sleep for a random amount of time.
                let max_sleep_interval =
                    self.ctx.max_sleep_interval(&self.chain_id)?;
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
