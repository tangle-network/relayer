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
//
use super::{BlockNumberOf, SubstrateEventWatcher};
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;
use ethereum_types::H256;
use std::sync::Arc;
use webb::evm::ethers::types;
use webb::substrate::protocol_substrate_runtime::api::anchor_bn254;
use webb::substrate::{protocol_substrate_runtime, subxt};

/// An Substrate Anchor Leaves Watcher that watches for Deposit events and save the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.
#[derive(Clone, Debug, Default)]
pub struct SubstrateAnchorLeavesWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for SubstrateAnchorLeavesWatcher {
    const TAG: &'static str = "Substrate leaves Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = protocol_substrate_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;

    type Event = anchor_bn254::events::Deposit;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // fetch chain_id
        let chain_id =
            api.constants().linkable_tree_bn254().chain_identifier()?;
        // fetch leaf_index from merkle tree at given block_number
        let at_hash = api
            .storage()
            .system()
            .block_hash(block_number, None)
            .await?;
        let next_leaf_index = api
            .storage()
            .merkle_tree_bn254()
            .next_leaf_index(event.tree_id, Some(at_hash))
            .await?;
        let leaf_index = next_leaf_index - 1;
        let chain_id = types::U256::from(chain_id);
        let tree_id = event.tree_id.to_string();
        let leaf = event.leaf;
        let value = (leaf_index, H256::from_slice(&leaf.0));
        store.insert_leaves((chain_id, tree_id.clone()), &[value])?;
        store.insert_last_deposit_block_number(
            (chain_id, tree_id.clone()),
            types::U64::from(block_number),
        )?;
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::LeavesStore,
            chain_id = %chain_id,
            leaf_index = %leaf_index,
            leaf = %value.1,
            tree_id = %tree_id,
            block_number = %block_number
        );
        Ok(())
    }
}
