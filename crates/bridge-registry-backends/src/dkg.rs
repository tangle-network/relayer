use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::tangle_runtime::api::runtime_types::pallet_bridge_registry::types::BridgeMetadata;
use webb_proposals::ResourceId;

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
        let storage = RuntimeApi::storage().bridge_registry();
        let next_bridge_index = storage.next_bridge_index();
        Ok(self
            .client
            .storage()
            .at_latest()
            .await?
            .fetch(&next_bridge_index)
            .await?
            .unwrap_or(1))
    }

    async fn resource_to_bridge_index(
        &self,
        resource_id: &ResourceId,
    ) -> Option<u32> {
        let resource_id2 = webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::header::ResourceId(resource_id.0);
        let storage = RuntimeApi::storage().bridge_registry();
        let resource_to_bridge_index =
            storage.resource_to_bridge_index(resource_id2);
        self.client
            .storage()
            .at_latest()
            .await
            .ok()?
            .fetch(&resource_to_bridge_index)
            .await
            .ok()?
    }

    async fn bridges(
        &self,
        index: u32,
    ) -> webb_relayer_utils::Result<Option<BridgeMetadata>> {
        let storage = RuntimeApi::storage().bridge_registry();
        let bridges = storage.bridges(index);
        Ok(self
            .client
            .storage()
            .at_latest()
            .await?
            .fetch(&bridges)
            .await?)
    }
}
