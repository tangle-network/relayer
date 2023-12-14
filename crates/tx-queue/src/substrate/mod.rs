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

mod substrate_tx_queue;
#[doc(hidden)]
pub use substrate_tx_queue::*;
use subxt_signer::sr25519::Keypair as Sr25519Pair;
use webb::substrate::subxt::{self, OnlineClient};
use webb_relayer_utils::Result;

/// Config trait for Substrate tx queue.
#[async_trait::async_trait]
pub trait SubstrateTxQueueConfig {
    /// Maximum number of milliseconds to wait before dequeuing a transaction from
    /// the queue.
    fn max_sleep_interval(&self, chain_id: u32) -> Result<u64>;
    /// Returns a Substrate client.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain Id.
    async fn substrate_provider<C: subxt::Config>(
        &self,
        chain_id: u32,
    ) -> Result<OnlineClient<C>>;
    /// Returns a Substrate wallet.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain Id.
    async fn substrate_wallet(&self, chain_id: u32) -> Result<Sr25519Pair>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use subxt_signer::sr25519::dev;
    use webb::substrate::tangle_runtime::api as RuntimeApi;
    use webb_relayer_store::queue::{QueueItem, QueueStore};
    use webb_relayer_store::sled::SledQueueKey;
    use webb_relayer_store::SledStore;
    use webb_relayer_types::suri::Suri;
    use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;
    use webb_relayer_utils::TangleRuntimeConfig;

    use super::*;

    pub fn setup_tracing() -> tracing::subscriber::DefaultGuard {
        // Setup tracing for tests
        let env_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(
                tracing_subscriber::filter::LevelFilter::DEBUG.into(),
            )
            .from_env_lossy();
        let s = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_test_writer()
            .without_time()
            .with_target(false)
            .compact()
            .finish();
        tracing::subscriber::set_default(s)
    }

    pub struct TxQueueContext;

    #[async_trait::async_trait]
    impl SubstrateTxQueueConfig for TxQueueContext {
        fn max_sleep_interval(&self, _chain_id: u32) -> Result<u64> {
            Ok(7000_u64)
        }

        async fn substrate_provider<C: subxt::Config>(
            &self,
            _chain_id: u32,
        ) -> Result<OnlineClient<C>> {
            Ok(subxt::OnlineClient::<C>::new().await?)
        }

        async fn substrate_wallet(
            &self,
            _chain_id: u32,
        ) -> Result<Sr25519Pair> {
            Ok(Suri(dev::alice()).into())
        }
    }

    #[tokio::test]
    #[ignore = "needs substrate node"]
    async fn should_handle_many_txs() -> webb_relayer_utils::Result<()> {
        let _guard = setup_tracing();
        let chain_id = 1081u32;

        let context = TxQueueContext;
        let store = SledStore::temporary()?;
        let client = context
            .substrate_provider::<TangleRuntimeConfig>(chain_id)
            .await?;
        let store = Arc::new(store);
        let tx_queue = SubstrateTxQueue::new(context, chain_id, store.clone());
        let _handle = tokio::spawn(tx_queue.run::<TangleRuntimeConfig>());
        let tx_count = 5;
        let tx_api = RuntimeApi::tx().system();
        let meatadata = client.metadata();
        for i in 0..tx_count {
            let tx = tx_api
                .remark_with_event(format!("tx {}", i).as_bytes().to_vec());
            let tx = TypeErasedStaticTxPayload::try_from((&meatadata, tx))?;
            let tx_key = SledQueueKey::from_substrate_chain_id(chain_id);
            let item = QueueItem::new(tx);
            QueueStore::enqueue_item(&store, tx_key, item)?;
        }
        // Wait for txs to be processed.
        tokio::time::sleep(tokio::time::Duration::from_secs(120)).await;
        Ok(())
    }
}
