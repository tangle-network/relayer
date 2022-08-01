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
use crate::context::RelayerContext;
use crate::store::sled::SledQueueKey;
use crate::store::QueueStore;
use anyhow::Context;
use ethereum_types::U256;
use futures::TryFutureExt;
use rand::Rng;
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::Duration;
use subxt::rpc::SubstrateTransactionStatus as TransactionStatus;

use webb::substrate::scale::{Compact, Encode};
use webb::substrate::subxt::extrinsic::ExtrinsicParams;
use webb::substrate::subxt::extrinsic::Signer;
use webb::substrate::subxt::sp_core::blake2_256;
use webb::substrate::subxt::sp_core::sr25519::Pair;
use webb::substrate::subxt::{self, PairSigner};

/// The SubstrateTxQueue stores transaction call params in bytes so the relayer can process them later.
/// This prevents issues such as creating transactions with the same nonce.
/// Randomized sleep intervals are used to prevent relayers from submitting
/// the same transaction.
#[derive(Clone)]
pub struct SubstrateTxQueue<S>
where
    S: QueueStore<Vec<u8>, Key = SledQueueKey>,
{
    ctx: RelayerContext,
    chain_id: U256,
    store: Arc<S>,
}
impl<S> SubstrateTxQueue<S>
where
    S: QueueStore<Vec<u8>, Key = SledQueueKey>,
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
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::tx_queue::SubstrateTxQueue;
    /// let tx_queue = SubstrateTxQueue::new(ctx, chain_name.clone(), store);
    /// ```

    pub fn new(ctx: RelayerContext, chain_id: U256, store: Arc<S>) -> Self {
        Self {
            ctx,
            chain_id,
            store,
        }
    }
    /// Starts the SubstrateTxQueue service.
    ///
    /// Returns a future that resolves `Ok(())` on success, otherwise returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::tx_queue::SubstrateTxQueue;
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
    #[tracing::instrument(skip_all, fields(node = %self.chain_id))]
    pub async fn run<X>(self) -> Result<(), anyhow::Error>
    where
        X: subxt::extrinsic::ExtrinsicParams<subxt::DefaultConfig>,
        <X as ExtrinsicParams<subxt::DefaultConfig>>::OtherParams: Default,
    {
        let chain_config = self
            .ctx
            .config
            .substrate
            .get(&self.chain_id.to_string())
            .context("Chain not configured")?;
        let chain_id = self.chain_id;
        let store = self.store;
        let backoff = backoff::ExponentialBackoff {
            max_elapsed_time: None,
            ..Default::default()
        };
        //  protocol-substrate client
        let client = self
            .ctx
            .substrate_provider::<subxt::DefaultConfig>(&chain_id.to_string())
            .await?;

        // get pair
        let pair = self.ctx.substrate_wallet(&chain_id.to_string()).await?;
        let signer: PairSigner<subxt::DefaultConfig, Pair> =
            PairSigner::new(pair);

        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::TxQueue,
            ty = "SUBSTRATE",
            chain_id = %chain_id.as_u64(),
            starting = true,
        );

        let task = || async {
            loop {
                tracing::debug!("Checking for any txs in the queue ...");
                // dequeue transaction call data. This are call params stored as bytes
                let maybe_call_data = store.dequeue_item(
                    SledQueueKey::from_substrate_chain_id(chain_id),
                )?;
                if let Some(call_data) = maybe_call_data {
                    let call_data = subxt::Encoded(call_data);
                    tracing::trace!(
                        "Transaction call in hex : {:?}",
                        hex::encode(&call_data.encode())
                    );
                    // This are the steps to create encoded extrinsic which can be executed by rpc client
                    let account_nonce = if let Some(nonce) = signer.nonce() {
                        nonce
                    } else {
                        client
                            .rpc()
                            .system_account_next_index(signer.account_id())
                            .map_err(anyhow::Error::from)
                            .await?
                    };

                    // 1.Construct our custom additional/extra params.
                    let additional_and_extra_params = {
                        // Obtain spec version and transaction version from the runtime version of the client.
                        let runtime = client
                            .rpc()
                            .runtime_version(None)
                            .map_err(anyhow::Error::from)
                            .await?;
                        X::new(
                            runtime.spec_version,
                            runtime.transaction_version,
                            account_nonce,
                            *client.genesis(),
                            Default::default(),
                        )
                    };

                    // 2. Construct signature. This is compatible with the Encode impl
                    //    for SignedPayload (which is this payload of bytes that we'd like)
                    //    to sign. See:
                    //    https://github.com/paritytech/substrate/blob/9a6d706d8db00abb6ba183839ec98ecd9924b1f8/primitives/runtime/src/generic/unchecked_extrinsic.rs#L215)
                    let signature = {
                        let mut bytes = Vec::new();
                        call_data.encode_to(&mut bytes);
                        additional_and_extra_params.encode_extra_to(&mut bytes);
                        additional_and_extra_params
                            .encode_additional_to(&mut bytes);
                        if bytes.len() > 256 {
                            signer.sign(&blake2_256(&bytes))
                        } else {
                            signer.sign(&bytes)
                        }
                    };

                    // 3. Encode extrinsic, now that we have the parts we need. This is compatible
                    //    with the Encode impl for UncheckedExtrinsic (protocol version 4).
                    let extrinsic = {
                        let mut encoded_inner = Vec::new();
                        // "is signed" + transaction protocol version (4)
                        (0b10000000 + 4u8).encode_to(&mut encoded_inner);
                        // from address for signature
                        signer.address().encode_to(&mut encoded_inner);
                        // the signature bytes
                        signature.encode_to(&mut encoded_inner);
                        // attach custom extra params
                        additional_and_extra_params
                            .encode_extra_to(&mut encoded_inner);
                        // and now, call data
                        call_data.encode_to(&mut encoded_inner);
                        // now, prefix byte length:
                        let len = Compact(
                            u32::try_from(encoded_inner.len())
                                .expect("extrinsic size expected to be <4GB"),
                        );
                        let mut encoded = Vec::new();
                        len.encode_to(&mut encoded);
                        encoded.extend(encoded_inner);
                        encoded
                    };
                    // encoded extinsic
                    let encoded_extrinsic = subxt::Encoded(extrinsic);
                    // watch_extrinsic submits and returns transaction subscription
                    let mut progress = client
                        .rpc()
                        .watch_extrinsic(&encoded_extrinsic)
                        .map_err(anyhow::Error::from)
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
                            TransactionStatus::Future => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Future",
                                );
                            }
                            TransactionStatus::Ready => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Ready",
                                );
                            }
                            TransactionStatus::Broadcast(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Broadcast",
                                );
                            }
                            TransactionStatus::InBlock(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "InBlock",
                                );
                            }
                            TransactionStatus::Retracted(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Retracted",
                                );
                            }
                            TransactionStatus::FinalityTimeout(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "FinalityTimeout",
                                );
                            }
                            TransactionStatus::Finalized(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Finalized",
                                    finalized = true,
                                );
                                // TODO wait for transaction success
                            }

                            TransactionStatus::Usurped(_) => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Usurped",
                                );
                            }
                            TransactionStatus::Dropped => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
                                    status = "Dropped",
                                );
                            }
                            TransactionStatus::Invalid => {
                                tracing::event!(
                                    target: crate::probe::TARGET,
                                    tracing::Level::DEBUG,
                                    kind = %crate::probe::Kind::TxQueue,
                                    ty = "SUBSTRATE",
                                    chain_id = %chain_id.as_u64(),
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
        backoff::future::retry::<(), _, _, _, _>(backoff, task).await?;
        Ok(())
    }
}
