use std::sync::Arc;

use tokio::sync::Mutex;
use webb::substrate::subxt;
use webb_event_watcher_traits::substrate::EventHandler;
use webb_relayer_utils::metric;

#[derive(Debug, Clone)]
pub struct PublicKeyChangedHandler {
    webb_config: webb_relayer_config::WebbRelayerConfig,
}

impl PublicKeyChangedHandler {
    pub fn new(webb_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { webb_config }
    }
}

#[async_trait::async_trait]
impl EventHandler<subxt::PolkadotConfig> for PublicKeyChangedHandler {
    type Client = subxt::OnlineClient<subxt::PolkadotConfig>;
    type Store = webb_relayer_store::SledStore;

    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        events: subxt::events::Events<subxt::PolkadotConfig>,
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        todo!()
    }
}
