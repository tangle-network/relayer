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
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::protocol_solidity::{
    SignatureBridgeContract, SignatureBridgeContractEvents,
};
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::evm::ethers::utils;

use crate::config;
use crate::events_watcher::{BridgeWatcher, EventWatcher};
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::{BridgeCommand, QueueStore};

type HttpProvider = providers::Provider<providers::Http>;

/// A Wrapper around the `SignatureBridgeContract` contract.
#[derive(Clone, Debug)]
pub struct SignatureBridgeContractWrapper<M: Middleware> {
    config: config::SignatureBridgeContractConfig,
    contract: SignatureBridgeContract<M>,
}

impl<M: Middleware> SignatureBridgeContractWrapper<M> {
    pub fn new(
        config: config::SignatureBridgeContractConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: SignatureBridgeContract::new(
                config.common.address,
                client,
            ),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for SignatureBridgeContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract
    for SignatureBridgeContractWrapper<M>
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

/// A SignatureBridge contract events & commands watcher.
#[derive(Copy, Clone, Debug, Default)]
pub struct SignatureBridgeContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for SignatureBridgeContractWatcher {
    const TAG: &'static str = "Signature Bridge Watcher";

    type Middleware = HttpProvider;

    type Contract = SignatureBridgeContractWrapper<Self::Middleware>;

    type Events = SignatureBridgeContractEvents;

    type Store = SledStore;

    #[tracing::instrument(
        skip_all,
        fields(event_type = ?e.0),
    )]
    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _wrapper: &Self::Contract,
        e: (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        tracing::debug!("Got Event {:?}", e.0);
        Ok(())
    }
}

#[async_trait::async_trait]
impl BridgeWatcher for SignatureBridgeContractWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        cmd: BridgeCommand,
    ) -> anyhow::Result<()> {
        use BridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            ExecuteProposalWithSignature { data, signature } => {
                self.execute_proposal_with_signature(
                    store,
                    &wrapper.contract,
                    (data, signature),
                )
                .await?;
            }
        };
        Ok(())
    }
}

impl SignatureBridgeContractWatcher
where
    Self: BridgeWatcher,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal_with_signature(
        &self,
        store: Arc<<Self as EventWatcher>::Store>,
        contract: &SignatureBridgeContract<<Self as EventWatcher>::Middleware>,
        (data, signature): (Vec<u8>, Vec<u8>),
    ) -> anyhow::Result<()> {
        // before doing anything, we need to do just two things:
        // 1. check if we already have this transaction in the queue.
        // 2. if not, check if the signature is valid.

        let chain_id = contract.get_chain_id().call().await?;
        let data_hash = utils::keccak256(&data);
        let tx_key = SledQueueKey::from_evm_with_custom_key(
            chain_id,
            make_execute_proposal_key(data_hash),
        );

        // check if we already have a queued tx for this proposal.
        // if we do, we should not enqueue it again.
        let qq = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
        if qq {
            tracing::debug!(
                data_hash = ?hex::encode(data_hash),
                "Skipping execution of the proposal since it is already in tx queue",
            );
            return Ok(());
        }

        // now we need to check if the signature is valid.
        let (data_clone, signature_clone) = (data.clone(), signature.clone());
        let is_signature_valid = contract
            .is_signature_from_governor(
                data_clone.into(),
                signature_clone.into(),
            )
            .call()
            .await?;

        let data_hex = hex::encode(&data);
        let signature_hex = hex::encode(&signature);
        if !is_signature_valid {
            tracing::warn!(
                data = ?data_hex,
                signature = ?signature_hex,
                "Skipping execution of this proposal since signature is invalid",
            );
            return Ok(());
        }

        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SignatureBridge,
            call = "execute_proposal_with_signature",
            chain_id = %chain_id.as_u64(),
            data = ?data_hex,
            signature = ?signature_hex,
            data_hash = ?hex::encode(data_hash),
        );
        // I guess now we are ready to enqueue the transaction.
        let call = contract
            .execute_proposal_with_signature(data.into(), signature.into());
        QueueStore::<TypedTransaction>::enqueue_item(&store, tx_key, call.tx)?;
        tracing::debug!(
            data_hash = ?hex::encode(data_hash),
            "Enqueued the proposal for execution in the tx queue",
        );
        Ok(())
    }
}

fn make_execute_proposal_key(data_hash: [u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let prefix = b"execute_proposal_with_signature_";
    result[0..32].copy_from_slice(prefix);
    result[32..64].copy_from_slice(&data_hash);
    result
}
