use typed_builder::TypedBuilder;
use webb::substrate::dkg_runtime::api::runtime_types::pallet_bridge_registry::types::{BridgeInfo, BridgeMetadata, SerdeData};
use webb::substrate::dkg_runtime::api::runtime_types::pallet_identity::types::Data;
use webb::substrate::scale::DecodeAll;
use crate::BridgeRegistryBackend;use hex_literal::hex;
use webb::substrate::dkg_runtime::api::runtime_types::sp_core::bounded::bounded_vec::BoundedVec;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::ResourceId;

#[derive(TypedBuilder)]
pub struct MockedBridgeRegistryBackend {}

#[async_trait::async_trait]
impl BridgeRegistryBackend for MockedBridgeRegistryBackend {
    async fn next_bridge_index(&self) -> webb_relayer_utils::Result<u32> {
        Ok(2)
    }

    async fn resource_to_bridge_index(
        &self,
        _resource_id: &webb_proposals::ResourceId,
    ) -> Option<u32> {
        Some(1)
    }

    async fn bridges(
        &self,
        _index: u32,
    ) -> webb_relayer_utils::Result<Option<BridgeMetadata>> {
        let display = SerdeData(
            Data::decode_all(&mut "mock bridge".as_bytes())
                .expect("decode mock bridge data"),
        );
        Ok(Some(BridgeMetadata {
            resource_ids: BoundedVec(vec![
                ResourceId(hex!("0000000000000000e69a847cd5bc0c9480ada0b339d7f0a8cac2b6670000138a")),
                ResourceId(hex!("000000000000d30c8839c1145609e564b986f667b273ddcb8496010000001389"))]),
            info: BridgeInfo { additional: BoundedVec(vec![]), display },
        }))
    }
}
