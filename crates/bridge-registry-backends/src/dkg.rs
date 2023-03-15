use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb::substrate::dkg_runtime::api as RuntimeApi;
use webb::substrate::dkg_runtime::api::runtime_types::pallet_bridge_registry::types::BridgeMetadata;

pub struct DkgBridgeRegistryBackend {
    client: OnlineClient<PolkadotConfig>,
}

impl DkgBridgeRegistryBackend {
    pub fn new(client: OnlineClient<PolkadotConfig>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl super::BridgeRegistryBackend for DkgBridgeRegistryBackend {
    async fn next_bridge_index(&self) -> webb_relayer_utils::Result<u32> {
        // retrieve resource ids from bridge registry
        let storage = RuntimeApi::storage().bridge_registry();
        let next_bridge_index = storage.next_bridge_index();
        Ok(self
            .client
            .storage()
            .at(None)
            .await?
            .fetch(&next_bridge_index)
            .await?
            .unwrap())
    }

    async fn bridges(
        &self,
        index: u32,
    ) -> webb_relayer_utils::Result<BridgeMetadata> {
        let storage = RuntimeApi::storage().bridge_registry();
        let bridges = storage.bridges(dbg!(index));
        Ok(self
            .client
            .storage()
            .at(None)
            .await?
            .fetch(&bridges)
            .await?
            .unwrap())
    }
}
