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

use futures::StreamExt;
use futures::TryFutureExt;
use rand::Rng;
use webb::substrate::subxt;
use webb::substrate::subxt::config::ExtrinsicParams;
use webb::substrate::subxt::PolkadotConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_store::queue::QueueStore;
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;

use std::sync::Arc;
use std::time::Duration;

use sp_core::sr25519;
use webb::substrate::subxt::tx::TxStatus as TransactionStatus;

/// The SubstrateTxQueue stores transaction call params in bytes so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct SubstrateTxQueue<S>
where
    S: QueueStore<TypeErasedStaticTxPayload, Key = SledQueueKey>,
{
    ctx: RelayerContext,
    chain_id: u32,
    store: Arc<S>,
}

impl<S> SubstrateTxQueue<S>
where
    S: QueueStore<TypeErasedStaticTxPayload, Key = SledQueueKey>,
{
    /// Creates a new SubstrateTxQueue instance.
    ///
    /// Returns a SubstrateTxQueue instance.
    ///
    /// # Arguments
    ///
    /// * `ctx` - RelayContext reference that holds the configuration
    /// * `chain_name` - The name of the chain that this queue is for
    /// * `store` - [Sled](https://sled.rs)-based database store
    pub fn new(ctx: RelayerContext, chain_id: u32, store: Arc<S>) -> Self {
        Self {
            ctx,
            chain_id,
            store,
        }
    }
    /// Starts the SubstrateTxQueue service.
    ///
    /// Returns a future that resolves `Ok(())` on success, otherwise returns an error.
    #[tracing::instrument(skip_all, fields(node = %self.chain_id))]
    pub async fn run<X>(self) -> webb_relayer_utils::Result<()>
    where
        X: subxt::Config,
        <<X>::ExtrinsicParams as ExtrinsicParams<<X>::Index, <X>::Hash>>::OtherParams:Default,
        <X>::Signature: From<sr25519::Signature>,
        <X>::Address: From<<X>::AccountId>,
        <X as subxt::Config>::AccountId: From<sp_runtime::AccountId32>
    {
        let chain_config = self
            .ctx
            .config
            .substrate
            .get(&self.chain_id.to_string())
            .ok_or(webb_relayer_utils::Error::NodeNotFound {
                chain_id: self.chain_id.to_string(),
            })?;
        let chain_id = self.chain_id;
        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };

        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::TxQueue,
            ty = "SUBSTRATE",
            chain_id = %chain_id,
            starting = true,
        );

        let metrics_clone = self.ctx.metrics.clone();
        let task = || async {
            //  Tangle node connection
            let maybe_client = self
                .ctx
                .substrate_provider::<PolkadotConfig, _>(chain_id)
                .await;
            let client = match maybe_client {
                Ok(client) => client,
                Err(err) => {
                    tracing::error!(
                        "Failed to connect with substrate client for chain_id: {}, retrying...!",
                        chain_id
                    );
                    return Err(backoff::Error::transient(err));
                }
            };
            let pair = self.ctx.substrate_wallet(chain_id).await?;
            let signer = subxt::tx::PairSigner::<PolkadotConfig, _>::new(pair);
            loop {
                // dequeue signed transaction
                let maybe_item = store.dequeue_item(
                    SledQueueKey::from_substrate_chain_id(chain_id),
                )?;
                if let Some(item) = maybe_item {
                    let payload = item.inner;
                    let signed_extrinsic = client
                        .tx()
                        .create_signed(&payload, &signer, Default::default())
                        .map_err(Into::into)
                        .map_err(backoff::Error::transient)
                        .await?;
                    // dry run test
                    let dry_run_outcome = signed_extrinsic.dry_run(None).await;
                    match dry_run_outcome {
                        Ok(_) => {
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "SUBSTRATE",
                                chain_id = %chain_id,
                                tx = %payload,
                                dry_run = "passed"
                            );
                        }
                        Err(err) => {
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "SUBSTRATE",
                                chain_id = %chain_id,
                                tx = %payload,
                                errored = true,
                                error = %err,
                                dry_run = "failed"
                            );
                            continue; // keep going.
                        }
                    }
                    // watch_extrinsic submits and returns transaction subscription
                    let mut progress = signed_extrinsic
                        .submit_and_watch()
                        .inspect_err(|e| {
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                ty = "SUBSTRATE",
                                chain_id = %chain_id,
                                tx = %payload,
                                errored = true,
                                error = %e,
                                progress = "failed",
                            );
                        })
                        .map_err(Into::into)
                        .map_err(backoff::Error::transient)
                        .await?;

                    while let Some(event) = progress.next().await {
                        let e = match event {
                            Ok(e) => e,
                            Err(err) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id,
                                    tx = %payload,
                                    errored = true,
                                    error = %err,
                                );
                                continue; // keep going.
                            }
                        };

                        match e {
                            TransactionStatus::Future => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Future",
                                );
                            }
                            TransactionStatus::Ready => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Ready",
                                );
                            }
                            TransactionStatus::Broadcast(_) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Broadcast",
                                );
                            }
                            TransactionStatus::InBlock(data) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    block_hash = ?data.block_hash(),
                                    status = "InBlock",
                                );
                            }
                            TransactionStatus::Retracted(_) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    tx = %payload,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id,
                                    status = "Retracted",
                                );
                            }
                            TransactionStatus::FinalityTimeout(_) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    tx = %payload,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id,
                                    status = "FinalityTimeout",
                                );
                            }
                            TransactionStatus::Finalized(_) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Finalized",
                                    finalized = true,
                                );
                                // metrics for proposal processed by substrate tx queue
                                let metrics = metrics_clone.lock().await;
                                metrics.proposals_processed_tx_queue.inc();
                                metrics
                                    .proposals_processed_substrate_tx_queue
                                    .inc();
                            }

                            TransactionStatus::Usurped(_) => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Usurped",
                                );
                            }
                            TransactionStatus::Dropped => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Dropped",
                                );
                            }
                            TransactionStatus::Invalid => {
                                tracing::event!(
                                    target: webb_relayer_utils::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %webb_relayer_utils::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    tx = %payload,
                                    chain_id = %chain_id,
                                    status = "Invalid",
                                );
                            }
                        }
                    }
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
        metrics.substrate_transaction_queue_back_off.inc();
        drop(metrics);
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
