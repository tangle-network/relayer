use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use futures::TryFutureExt;
use rand::Rng;
use webb::evm::ethers::prelude::*;

use crate::context::RelayerContext;
use crate::store::TxQueueStore;
use crate::utils::ClickableLink;

#[derive(Clone)]
pub struct TxQueue<S: TxQueueStore> {
    ctx: RelayerContext,
    chain_name: String,
    store: Arc<S>,
}

impl<S: TxQueueStore> TxQueue<S> {
    pub fn new(ctx: RelayerContext, chain_name: String, store: Arc<S>) -> Self {
        Self {
            ctx,
            chain_name,
            store,
        }
    }

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
                tracing::debug!("Checking for any txs in the queue ...");
                let maybe_tx = store.dequeue_tx(chain_id)?;
                let maybe_explorer = &chain_config.explorer;
                let tx_hash: H256;
                if let Some(tx) = maybe_tx {
                    let pending_tx = client
                        .send_transaction(tx, None)
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
                                .interval(Duration::from_millis(7000))
                                .await;
                            result
                        }
                        Err(e) => {
                            tracing::error!("Error while sending Tx: {}", e);
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
                let s = rand::thread_rng().gen_range(5..=60);
                tracing::debug!("next queue round after {} seconds", s);
                tokio::time::sleep(Duration::from_secs(s)).await;
            }
        };
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
