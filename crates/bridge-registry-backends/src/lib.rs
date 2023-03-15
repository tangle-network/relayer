use ethereum_types::H256;
use webb::substrate::dkg_runtime::api::runtime_types::pallet_bridge_registry::types::BridgeMetadata;
use webb_proposals::ResourceId;
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_config::anchor::RawResourceId;

#[doc(hidden)]
pub mod dkg;

#[doc(hidden)]
pub mod mocked;

#[async_trait::async_trait]
pub trait BridgeRegistryBackend {
    async fn next_bridge_index(&self) -> webb_relayer_utils::Result<u32>;
    async fn resource_to_bridge_id(
        &self,
        resource_id: &ResourceId,
    ) -> webb_relayer_utils::Result<u32>;
    async fn bridges(
        &self,
        index: u32,
    ) -> webb_relayer_utils::Result<Option<BridgeMetadata>>;

    async fn config_or_dkg_bridges(
        &self,
        anchors: &Option<Vec<LinkedAnchorConfig>>,
        resource_id: &ResourceId,
    ) -> webb_relayer_utils::Result<Vec<LinkedAnchorConfig>> {
        match anchors {
            Some(a) => Ok(a.clone()),
            None => {
                let next_bridge_index =
                    self.resource_to_bridge_id(resource_id).await?;
                let bridges = self.bridges(next_bridge_index).await?.unwrap();
                Ok(bridges
                    .resource_ids
                    .0
                    .into_iter()
                    .filter(|r| r.0 != resource_id.0)
                    .map(|r| {
                        let rr = RawResourceId {
                            resource_id: H256(r.0),
                        };
                        LinkedAnchorConfig::Raw(rr)
                    })
                    .collect())
            }
        }
    }
}
