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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use subxt_signer::sr25519::dev;
    use webb::substrate::tangle_runtime::api as RuntimeApi;
    use webb_relayer_store::queue::{QueueItem, QueueStore};
    use webb_relayer_store::sled::SledQueueKey;
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

    #[tokio::test]
    #[ignore = "needs substrate node"]
    async fn should_handle_many_txs() -> webb_relayer_utils::Result<()> {
        let _guard = setup_tracing();
        let chain_id = 1081u32;
        let config = webb_relayer_config::WebbRelayerConfig {
            substrate: HashMap::from([(
                chain_id.to_string(),
                webb_relayer_config::substrate::SubstrateConfig {
                    name: String::from("tangle"),
                    enabled: true,
                    http_endpoint: "http://localhost:9933"
                        .parse::<url::Url>()
                        .unwrap()
                        .into(),
                    ws_endpoint: "ws://localhost:9944"
                        .parse::<url::Url>()
                        .unwrap()
                        .into(),
                    explorer: None,
                    chain_id,
                    suri: Some(Suri(dev::alice())),
                    pallets: Default::default(),
                    tx_queue: Default::default(),
                },
            )]),
            ..Default::default()
        };
        let store = webb_relayer_store::SledStore::temporary()?;
        let context =
            webb_relayer_context::RelayerContext::new(config, store.clone())?;
        let client = context
            .substrate_provider::<TangleRuntimeConfig, _>(chain_id)
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
