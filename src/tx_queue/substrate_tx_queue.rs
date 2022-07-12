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
use ethereum_types::U256;
use futures::TryFutureExt;
use rand::Rng;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;

use crate::context::RelayerContext;
use crate::store::sled::SledQueueKey;
use crate::store::QueueStore;
use crate::utils::ClickableLink;
use webb::substrate::scale::Decode;

use webb::substrate::{
    protocol_substrate_runtime,
    subxt::{self, PairSigner, HasModuleError, Call},
};

/// The TxQueue stores transaction requests so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct  SubstrateTxQueue<'a, S, T, X, C, E, Evs>
where
    S: QueueStore<subxt::SubmittableExtrinsic<'a, T, X, C, E, Evs>, Key = SledQueueKey>,
    T: subxt::Config,
    X: subxt::extrinsic::ExtrinsicParams<T>,
    C: Call + Send + Sync,
    E: Decode + HasModuleError,
    Evs: Decode,

{
    ctx: RelayerContext,
    node_name: String,
    chain_id: U256,
    store: Arc<S>,
}

impl<'a, S, T, X, C, E, Evs> SubstrateTxQueue<'a, S, T, X, C, E, Evs>
where
    S: QueueStore<subxt::SubmittableExtrinsic<'a, T, X, C, E, Evs>, Key = SledQueueKey>,
    T: subxt::Config,
    X: subxt::extrinsic::ExtrinsicParams<T>,
    C: Call + Send + Sync,
    E: Decode + HasModuleError,
    Evs: Decode,
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
    pub fn new(ctx: RelayerContext, node_name: String, chain_id: U256, store: Arc<S>) -> Self {
        Self {
            ctx,
            node_name,
            chain_id,
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
    #[tracing::instrument(skip_all, fields(node = %self.node_name))]
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let ctx = self.ctx;
        let node_name = self.node_name;
        let chain_id = self.chain_id;
        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };

        let chain_config = self
            .ctx
            .config
            .substrate
            .get(&self.node_name)
            .context("Chain not configured")?;

        // get pair
        let pair = ctx.substrate_wallet(&node_name).await?;
        let signer = PairSigner::new(pair);

        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::TxQueue,
            ty = "Substrate",
            chain_id = %chain_id.as_u64(),
            starting = true,
        );

        let task = || async {
            loop {
                tracing::trace!("Checking for any txs in the queue ...");
                // dequeue submitable extrinsic
                let tx = store
                    .dequeue_item(SledQueueKey::from_substrate_chain_id(chain_id))?;
                
                let mut progress = tx
                .sign_and_submit_then_watch_default(&signer)
                .await?;

                while let Some(event) = progress.next().await {
                    let e = match event {
                        Ok(e) => e,
                        Err(err) => {
                            tracing::event!(
                                target: crate::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %crate::probe::Kind::TxQueue,
                                ty = "SUBSTRATE",
                                chain_id = %chain_id.as_u64(),
                                errored = true,
                                error = %err,
                            );
                            continue; // keep going.
                        }
                    };
        
                    match e {
                        subxt::TransactionStatus::Future => {}
                        subxt::TransactionStatus::Ready => {
                            tracing::trace!("tx ready");
                        }
                        subxt::TransactionStatus::Broadcast(_) => {}
                        subxt::TransactionStatus::InBlock(_) => {
                            tracing::trace!("tx in block");
                        }
                        subxt::TransactionStatus::Retracted(_) => {
                            tracing::warn!("tx retracted");
                        }
                        subxt::TransactionStatus::FinalityTimeout(_) => {
                            tracing::warn!("tx timeout");
                        }
                        subxt::TransactionStatus::Finalized(v) => {
                            let maybe_success = v.wait_for_success().await;
                            match maybe_success {
                                Ok(events) => {
                                    tracing::debug!(?events, "tx finalized",);
                                }
                                Err(err) => {
                                    tracing::error!(error = %err, "tx failed");
                                    return Err(err.into());
                                }
                            }
                        }
                        subxt::TransactionStatus::Usurped(_) => {}
                        subxt::TransactionStatus::Dropped => {
                            tracing::warn!("tx dropped");
                        }
                        subxt::TransactionStatus::Invalid => {
                            tracing::warn!("tx invalid");
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
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
