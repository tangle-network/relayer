use ethereum_types::H256;
use webb::substrate::dkg_runtime::api::runtime_types::pallet_bridge_registry::types::BridgeMetadata;
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_config::anchor::RawResourceId;

#[doc(hidden)]
pub mod dkg;

#[doc(hidden)]
pub mod mocked;

#[async_trait::async_trait]
pub trait BridgeRegistryBackend {
    async fn next_bridge_index(&self) -> webb_relayer_utils::Result<u32>;
    async fn bridges(
        &self,
        index: u32,
    ) -> webb_relayer_utils::Result<BridgeMetadata>;

    async fn config_or_dkg_bridges(
        &self,
        anchors: &Option<Vec<LinkedAnchorConfig>>,
    ) -> webb_relayer_utils::Result<Vec<LinkedAnchorConfig>> {
        match anchors {
            Some(a) => Ok(a.clone()),
            None => {
                let next_bridge_index = self.next_bridge_index().await.unwrap();
                let mut linked_anchors = vec![];
                for i in 1..next_bridge_index {
                    let bridges = self.bridges(i).await.unwrap();
                    linked_anchors.append(&mut bridges.resource_ids.0.to_vec());
                }
                Ok(linked_anchors
                    .into_iter()
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
