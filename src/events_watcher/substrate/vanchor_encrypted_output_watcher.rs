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
use crate::metric;
use std::sync::Arc;
use webb::evm::ethers::types;
use webb::substrate::protocol_substrate_runtime;
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::protocol_substrate_runtime::api::v_anchor_bn254;
use webb::substrate::subxt::{self, OnlineClient};
use webb_relayer_store::sled::SledStore;
use webb_relayer_store::EncryptedOutputCacheStore;
// An Substrate VAnchor encrypted output Watcher that watches for Deposit events and save the encrypted output to the store.
/// It serves as a cache for encrypted outputs that could be used by dApp.
#[derive(Clone, Debug, Default)]
pub struct SubstrateVAnchorEncryptedOutputHandler;

#[async_trait::async_trait]
impl SubstrateEventWatcher for SubstrateVAnchorEncryptedOutputHandler {
    const TAG: &'static str = "Substrate V-Anchor encrypted output handler";

    type RuntimeConfig = subxt::SubstrateConfig;

    type Client = OnlineClient<Self::RuntimeConfig>;

    type Event = protocol_substrate_runtime::api::Event;

    type FilteredEvent = v_anchor_bn254::events::Transaction;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        _metrics: Arc<metric::Metrics>,
    ) -> crate::Result<()> {
        let at_hash_addr = RuntimeApi::storage()
            .system()
            .block_hash(block_number as u64);
        let at_hash = api.storage().fetch(&at_hash_addr, None).await?.unwrap();

        // fetch leaf_index from merkle tree at given block_number
        let next_leaf_index_addr = RuntimeApi::storage()
            .merkle_tree_bn254()
            .next_leaf_index(event.tree_id);
        let next_leaf_index = api
            .storage()
            .fetch(&next_leaf_index_addr, Some(at_hash))
            .await?
            .unwrap();

        // fetch chain_id
        let chain_id_addr = RuntimeApi::constants()
            .linkable_tree_bn254()
            .chain_identifier();
        let chain_id = api.constants().at(&chain_id_addr)?;
        let chain_id = types::U256::from(chain_id);

        let tree_id = event.tree_id.to_string();
        let leaf_count = event.leafs.len();

        let mut index = next_leaf_index.saturating_sub(leaf_count as u32);
        let mut output_store = Vec::with_capacity(leaf_count);
        for encrypted_output in
            [event.encrypted_output1, event.encrypted_output2]
        {
            let value = (index, encrypted_output.clone());
            store.insert_encrypted_output(
                (chain_id, tree_id.clone()),
                &[value],
            )?;
            store.insert_last_deposit_block_number_for_encrypted_output(
                (chain_id, tree_id.clone()),
                types::U64::from(block_number),
            )?;
            index += 1;
            output_store.push(encrypted_output);
        }
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::EncryptedOutputStore,
            chain_id = %chain_id,
            index = index,
            encrypted_outputs = %format!("{:?}",output_store),
            tree_id = %tree_id,
            block_number = %block_number
        );
        Ok(())
    }
}
