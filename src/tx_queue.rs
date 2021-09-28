use std::sync::Arc;
use std::time::Duration;

use futures::TryFutureExt;
use webb::evm::ethers::prelude::*;

use crate::context::RelayerContext;
use crate::store::TxQueueStore;

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

    pub async fn run(self) -> Result<(), anyhow::Error> {
        let provider = self.ctx.evm_provider(&self.chain_name).await?;
        let wallet = self.ctx.evm_wallet(&self.chain_name).await?;
        let client = Arc::new(SignerMiddleware::new(provider, wallet));
        let chain_id = client.get_chainid().await?;
        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        let task = || async {
            loop {
                let maybe_tx = store.dequeue_tx(chain_id)?;
                match maybe_tx {
                    Some(tx) => {
                        let pending_tx = client
                            .send_transaction(tx, None)
                            .map_err(anyhow::Error::from);
                        let tx = match pending_tx.await {
                            Ok(pending) => {
                                tracing::debug!(
                                    "Tx {} is submitted and pending!",
                                    *pending
                                );
                                let result = pending
                                    .interval(Duration::from_millis(7000))
                                    .await;
                                result
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error while sending Tx: {}",
                                    e
                                );
                                return Err(e.into());
                            }
                        };
                        match tx {
                            Ok(Some(receipt)) => {
                                tracing::debug!(
                                    "Tx {} Finalized",
                                    receipt.transaction_hash
                                );
                            }
                            Ok(None) => {
                                tracing::warn!("Tx Dropped from Mempool!!");
                                return Err(anyhow::anyhow!(
                                    "DroppedFromMemPool",
                                )
                                .into());
                            }
                            Err(e) => {
                                let reason = e.to_string();
                                tracing::error!("Tx Errored: {}", reason);
                            }
                        };
                    }
                    None => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        };
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
