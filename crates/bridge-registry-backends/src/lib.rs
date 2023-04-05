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

#![warn(missing_docs)]
//! # Relayer Bridge Registry Backends 🕸️
//!
//! ## Overview
//! This crate contains the bridge registry backends for the relayer.
//! Bridge registry backends are used to retrieve the bridges and attached resources that are
//! configured in relayer.
//! There are two types of bridge backends:
//! - `MockedBridgeRegistryBackend`: This is a mocked backend that is used for testing purposes.
//! - `DKGBridgeRegistryBackend`: This is the actual backend that is used in production.

use ethereum_types::H256;
use webb::substrate::dkg_runtime::api::runtime_types::pallet_bridge_registry::types::BridgeMetadata;
use webb_proposals::ResourceId;
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_config::anchor::RawResourceId;
use webb_relayer_utils::Error;

#[doc(hidden)]
pub mod dkg;

#[doc(hidden)]
pub mod mocked;

/// Wrapper for calling DKG bridge registry pallet methods.
#[async_trait::async_trait]
pub trait BridgeRegistryBackend {
    /// Returns the next available bridge index. In other words, it returns the index of the
    /// last bridge + 1.
    async fn next_bridge_index(&self) -> webb_relayer_utils::Result<u32>;

    /// Retrieves the bridge index which a given resource is attached to.
    ///
    /// The passed resource must belong to a bridge that is registered with DKG, otherwise an error
    /// is returned.
    async fn resource_to_bridge_index(
        &self,
        resource_id: &ResourceId,
    ) -> Option<u32>;

    /// Returns bridge with the given index, if any.
    async fn bridges(
        &self,
        index: u32,
    ) -> webb_relayer_utils::Result<Option<BridgeMetadata>>;

    /// Attempts to return other linked anchors from the same bridge as passed linked anchor.
    ///
    /// If `anchors` config value is passed, this is returned unconditionally (for use with
    /// mocks/testing).
    ///
    /// If `anchors` is `None`, uses [resource_to_bridge_id] to retrieve the matching bridge, then
    /// retrieves the bridge itself to get registered anchors. It returns all of them excluding the
    /// one that was passed as `linked_anchor` parameter.
    async fn config_or_dkg_bridges(
        &self,
        anchors: &Option<Vec<LinkedAnchorConfig>>,
        linked_anchor: &ResourceId,
    ) -> webb_relayer_utils::Result<Vec<LinkedAnchorConfig>> {
        match anchors {
            Some(a) => Ok(a.clone()),
            None => {
                let next_bridge_index = self
                    .resource_to_bridge_index(linked_anchor)
                    .await
                    .ok_or(Error::BridgeNotRegistered(*linked_anchor))?;
                match self.bridges(next_bridge_index).await? {
                    None => Ok(vec![]),
                    Some(bridges) => Ok(bridges
                        .resource_ids
                        .0
                        .into_iter()
                        .filter(|r| r.0 != linked_anchor.0)
                        .map(|r| {
                            let rr = RawResourceId {
                                resource_id: H256(r.0),
                            };
                            LinkedAnchorConfig::Raw(rr)
                        })
                        .collect()),
                }
            }
        }
    }
}
