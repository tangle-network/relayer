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

use crate::config;
use crate::events_watcher::signing_backend::SigningBackend;
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;

type HttpProvider = providers::Provider<providers::Http>;
/// Represents an Anchor Contract Watcher which will use the DKG Substrate nodes for signing.
pub struct AnchorWatcher<B> {
    signing_backend: B,
}

impl<B> AnchorWatcher<B>
where
    B: SigningBackend<webb_proposals::AnchorUpdateProposal>,
{
    pub fn new(signing_backend: B) -> Self {
        Self { signing_backend }
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
    B: SigningBackend<webb_proposals::AnchorUpdateProposal> + Send + Sync,
{
    const TAG: &'static str = "Anchor Watcher";
    type Middleware = HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

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
            );
            let can_sign_proposal =
                self.signing_backend.can_handle_proposal(&proposal).await?;
            if can_sign_proposal {
                self.signing_backend.handle_proposal(&proposal).await?;
            } else {
                tracing::warn!(
                    "Anchor update proposal is not supported by the signing backend"
                );
            }
        }
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
