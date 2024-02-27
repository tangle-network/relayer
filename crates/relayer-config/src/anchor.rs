// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

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
