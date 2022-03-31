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
use futures::StreamExt;
use webb::evm::contract::protocol_solidity::{
    FixedDepositAnchorContract, FixedDepositAnchorContractEvents,
};
use webb::evm::ethers::prelude::{Contract, LogMeta, Middleware};
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, Nonce, ResourceId};
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::{dkg_runtime, subxt};

use crate::config;
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;

type HttpProvider = providers::Provider<providers::Http>;
/// Represents an Anchor Contract Watcher which will use the DKG Substrate nodes for signing.
pub struct AnchorWatcherWithSubstrate<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    api: R,
    pair: subxt::PairSigner<C, subxt::DefaultExtra<C>, Sr25519Pair>,
}

impl<R, C> AnchorWatcherWithSubstrate<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    /// Creates a new AnchorWatcherWithSubstrate.
    pub fn new(
        client: subxt::Client<C>,
        pair: subxt::PairSigner<C, subxt::DefaultExtra<C>, Sr25519Pair>,
    ) -> Self {
        Self {
            api: client.to_runtime_api(),
            pair,
        }
    }
}

type DKGConfig = subxt::DefaultConfig;
type DKGRuntimeApi =
    dkg_runtime::api::RuntimeApi<DKGConfig, subxt::DefaultExtra<DKGConfig>>;
/// Type alias for the AnchorWatcherWithSubstrate.
pub type AnchorWatcherOverDKG =
    AnchorWatcherWithSubstrate<DKGRuntimeApi, DKGConfig>;

/// AnchorContractOverDKGWrapper contains FixedDepositAnchorContract contract along with configurations for Anchor contract over DKG, and Relayer.  
#[derive(Clone, Debug)]
pub struct AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    config: config::AnchorContractOverDKGConfig,
    webb_config: config::WebbRelayerConfig,
    contract: FixedDepositAnchorContract<M>,
}

impl<M> AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    /// Creates a new AnchorContractOverDKGWrapper.
    pub fn new(
        config: config::AnchorContractOverDKGConfig,
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

impl<M> ops::Deref for AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> super::WatchableContract for AnchorContractOverDKGWrapper<M>
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

#[derive(Copy, Clone, Debug, Default)]
pub struct AnchorLeavesWatcher;

#[async_trait::async_trait]
impl super::EventWatcher for AnchorWatcherOverDKG {
    const TAG: &'static str = "Anchor Watcher Over DKG";
    type Middleware = HttpProvider;

    type Contract = AnchorContractOverDKGWrapper<Self::Middleware>;

    type Events = FixedDepositAnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use FixedDepositAnchorContractEvents::*;
        // only process anchor deposit events.
        let event_data = match event {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
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
            // first we need to do some checks before sending the proposal.
            // 1. check if the origin_chain_id is whitleisted.
            let storage_api = self.api.storage().dkg_proposals();
            let maybe_whitelisted = storage_api
                .chain_nonces(TypedChainId::Evm(src_chain_id.as_u32()), None)
                .await?;
            if maybe_whitelisted.is_none() {
                // chain is not whitelisted.
                tracing::warn!(
                    "chain {} is not whitelisted, skipping proposal",
                    src_chain_id
                );
                continue;
            }
            // 2. check for the resource id if it exists or not.
            // if not, we need to skip the proposal.
            let maybe_resource_id = storage_api
                .resources(ResourceId(resource_id.into_bytes()), None)
                .await?;
            if maybe_resource_id.is_none() {
                // resource id doesn't exist.
                tracing::warn!(
                    "resource id 0x{} doesn't exist, skipping proposal",
                    hex::encode(resource_id.into_bytes())
                );
                continue;
            }
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
            );
            let tx_api = self.api.tx().dkg_proposals();
            tracing::debug!(
                "sending proposal = nonce: {}, r_id: 0x{}, proposal_data: 0x{}",
                leaf_index,
                hex::encode(resource_id.into_bytes()),
                hex::encode(&proposal.to_bytes())
            );
            let xt = tx_api.acknowledge_proposal(
                Nonce(leaf_index),
                TypedChainId::Evm(src_chain_id.as_u32()),
                ResourceId(resource_id.into_bytes()),
                proposal.to_bytes().into(),
            );
            let signer = &self.pair;
            let mut progress = xt.sign_and_submit_then_watch(signer).await?;
            while let Some(event) = progress.next().await {
                tracing::debug!("Tx Progress: {:?}", event);
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorLeavesWatcher {
    const TAG: &'static str = "Anchor Watcher For Leaves";

    type Middleware = HttpProvider;

    type Contract = AnchorContractOverDKGWrapper<Self::Middleware>;

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
                tracing::trace!(
                    %log.block_number,
                    "detected block number",
                );
                tracing::debug!(
                    "Saved Deposit Event ({}, {}) at block {}",
                    value.0,
                    value.1,
                    log.block_number
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
