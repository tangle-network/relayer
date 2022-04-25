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
#![warn(missing_docs)]
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use ethereum_types::H256;
use webb::evm::contract::protocol_solidity::{
    FixedDepositAnchorContract, FixedDepositAnchorContractEvents,
};
use webb::evm::ethers::prelude::{Contract, LogMeta, Middleware};
use webb::evm::ethers::providers;
use webb::evm::ethers::types;

use super::{BlockNumberOf, SubstrateEventWatcher};
use webb::substrate::protocol_substrate_runtime::api::anchor_bn254;
use webb::substrate::{protocol_substrate_runtime, subxt};

use crate::config;
use crate::events_watcher::proposal_signing_backend::ProposalSigningBackend;
use crate::store::sled::SledStore;
use crate::store::{EventHashStore, LeafCacheStore};

type HttpProvider = providers::Provider<providers::Http>;
/// Represents an Anchor Contract Watcher which will use a configured signing backend for signing proposals.
pub struct AnchorWatcher<B> {
    proposal_signing_backend: B,
}

impl<B> AnchorWatcher<B>
where
    B: ProposalSigningBackend<webb_proposals::AnchorUpdateProposal>,
{
    pub fn new(proposal_signing_backend: B) -> Self {
        Self {
            proposal_signing_backend,
        }
    }
}

/// AnchorContractWrapper contains FixedDepositAnchorContract contract along with configurations for Anchor contract, and Relayer.
#[derive(Clone, Debug)]
pub struct AnchorContractWrapper<M>
where
    M: Middleware,
{
    config: config::AnchorContractConfig,
    webb_config: config::WebbRelayerConfig,
    contract: FixedDepositAnchorContract<M>,
}

impl<M> AnchorContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new AnchorContractOverDKGWrapper.
    pub fn new(
        config: config::AnchorContractConfig,
        webb_config: config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: FixedDepositAnchorContract::new(
                config.common.address,
                client,
            ),
            config,
            webb_config,
        }
    }
}

impl<M> ops::Deref for AnchorContractWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> super::WatchableContract for AnchorContractWrapper<M>
where
    M: Middleware,
{
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }

    fn max_events_per_step(&self) -> types::U64 {
        self.config.events_watcher.max_events_per_step.into()
    }

    fn print_progress_interval(&self) -> Duration {
        Duration::from_millis(
            self.config.events_watcher.print_progress_interval,
        )
    }
}

/// An Anchor Leaves Watcher that watches for Deposit events and save the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.
#[derive(Copy, Clone, Debug, Default)]
pub struct AnchorLeavesWatcher;

#[async_trait::async_trait]
impl<B> super::EventWatcher for AnchorWatcher<B>
where
    B: ProposalSigningBackend<webb_proposals::AnchorUpdateProposal>
        + Send
        + Sync,
{
    const TAG: &'static str = "Anchor Watcher";
    type Middleware = HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = FixedDepositAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use FixedDepositAnchorContractEvents::*;
        // only process anchor deposit events.
        let event_data = match event {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
        tracing::debug!(
            event = ?event_data,
            "Anchor deposit event",
        );
        let client = wrapper.contract.client();
        let src_chain_id = client.get_chainid().await?;
        let root = wrapper.contract.get_last_root().call().await?;
        let leaf_index = event_data.leaf_index;
        let function_signature = [68, 52, 123, 169];
        let nonce = event_data.leaf_index;
        for linked_anchor in &wrapper.config.linked_anchors {
            let dest_chain = linked_anchor.chain.to_lowercase();
            let maybe_chain = wrapper.webb_config.evm.get(&dest_chain);
            let dest_chain = match maybe_chain {
                Some(chain) => chain,
                None => continue,
            };
            let target_system =
                webb_proposals::TargetSystem::new_contract_address(
                    linked_anchor.address.to_fixed_bytes(),
                );
            let typed_chain_id =
                webb_proposals::TypedChainId::Evm(dest_chain.chain_id as _);
            let resource_id =
                webb_proposals::ResourceId::new(target_system, typed_chain_id);
            let header = webb_proposals::ProposalHeader::new(
                resource_id,
                function_signature.into(),
                nonce.into(),
            );
            let proposal = webb_proposals::AnchorUpdateProposal::new(
                header,
                webb_proposals::TypedChainId::Evm(src_chain_id.as_u32()),
                leaf_index,
                root,
                target_system.into_fixed_bytes(),
            );
            let can_sign_proposal = self
                .proposal_signing_backend
                .can_handle_proposal(&proposal)
                .await?;
            if can_sign_proposal {
                self.proposal_signing_backend
                    .handle_proposal(&proposal)
                    .await?;
            } else {
                tracing::warn!(
                    "Anchor update proposal is not supported by the signing backend"
                );
            }
        }
        // mark this event as processed.

        let events_bytes = serde_json::to_vec(&event_data)?;
        store.store_event(&events_bytes)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorLeavesWatcher {
    const TAG: &'static str = "Anchor Watcher For Leaves";

    type Middleware = HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = FixedDepositAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use FixedDepositAnchorContractEvents::*;
        match event {
            DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                let value = (leaf_index, H256::from_slice(&commitment));
                let chain_id = wrapper.contract.client().get_chainid().await?;
                store.insert_leaves(
                    (chain_id, wrapper.contract.address()),
                    &[value],
                )?;
                store.insert_last_deposit_block_number(
                    (chain_id, wrapper.contract.address()),
                    log.block_number,
                )?;
                let events_bytes = serde_json::to_vec(&deposit)?;
                store.store_event(&events_bytes)?;
                tracing::trace!(
                    %log.block_number,
                    "detected block number",
                );
                tracing::event!(
                    target: crate::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %crate::probe::Kind::LeavesStore,
                    leaf_index = %value.0,
                    leaf = %value.1,
                    chain_id = %chain_id,
                    block_number = %log.block_number
                );
            }
            EdgeAdditionFilter(v) => {
                tracing::debug!(
                    "Edge Added of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(v.merkle_root)
                );
            }
            EdgeUpdateFilter(v) => {
                tracing::debug!(
                    "Edge Updated of chain {} at index {} with root 0x{}",
                    v.chain_id,
                    v.latest_leaf_index,
                    hex::encode(v.merkle_root)
                );
            }
            _ => {
                tracing::trace!("Unhandled event {:?}", event);
            }
        };

        Ok(())
    }
}

/// An Substrate Anchor Leaves Watcher that watches for Deposit events and save the leaves to the store.
/// It serves as a cache for leaves that could be used by dApp for proof generation.
#[derive(Clone, Debug, Default)]
pub struct SubstrateLeavesWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for SubstrateLeavesWatcher {
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
