use ethereum_types::H256;

use crate::evm::EvmLinkedAnchorConfig;

use super::*;

/// Linked anchor config for Evm based target system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct RawResourceId {
    /// Raw resource Id
    pub resource_id: H256,
}

/// LinkedAnchorConfig is configuration for the linked anchors. Linked anchor can be added in multiple ways
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LinkedAnchorConfig {
    /// Linked anchor configuration for raw resource Id   
    Raw(RawResourceId),
    /// Linked anchor configuration for evm based chains
    Evm(EvmLinkedAnchorConfig),
}

impl LinkedAnchorConfig {
    /// Convert linked anchor to Raw resource Id format
    pub fn into_raw_resource_id(self) -> LinkedAnchorConfig {
        match self {
            LinkedAnchorConfig::Evm(config) => {
                let target_system =
                    webb_proposals::TargetSystem::new_contract_address(
                        config.address,
                    );
                let typed_chain_id =
                    webb_proposals::TypedChainId::Evm(config.chain_id);
                let resource_id = webb_proposals::ResourceId::new(
                    target_system,
                    typed_chain_id,
                );
                let raw_resource_id = RawResourceId {
                    resource_id: H256::from_slice(
                        resource_id.to_bytes().as_slice(),
                    ),
                };
                LinkedAnchorConfig::Raw(raw_resource_id)
            }
            _ => self,
        }
    }
}
