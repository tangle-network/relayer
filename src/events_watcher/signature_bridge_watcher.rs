use std::ops;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use webb::evm::contract::protocol_solidity::{
    SignatureBridgeContract, SignatureBridgeContractEvents,
};
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::evm::ethers::utils;

use crate::config;
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::QueueStore;

use super::{BridgeWatcher, EventWatcher};

type HttpProvider = providers::Provider<providers::Http>;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposalDataWithSignature {
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum SignatureBridgeCommand {
    ExecuteProposal(ProposalDataWithSignature),
}

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

#[derive(Copy, Clone, Debug, Default)]
pub struct SignatureBridgeWatcher;

#[async_trait::async_trait]
impl EventWatcher for SignatureBridgeWatcher {
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
        tracing::trace!("Got Event {:?}", e.0);
        Ok(())
    }
}

#[async_trait::async_trait]
impl BridgeWatcher<SignatureBridgeCommand> for SignatureBridgeWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        cmd: SignatureBridgeCommand,
    ) -> anyhow::Result<()> {
        use SignatureBridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            ExecuteProposal(proposal) => {
                self.execute_proposal(store, &wrapper.contract, proposal)
                    .await?;
            }
        };
        Ok(())
    }
}

impl SignatureBridgeWatcher
where
    Self: BridgeWatcher<SignatureBridgeCommand>,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal(
        &self,
        store: Arc<<Self as EventWatcher>::Store>,
        contract: &SignatureBridgeContract<<Self as EventWatcher>::Middleware>,
        proposal: ProposalDataWithSignature,
    ) -> anyhow::Result<()> {
        let dest_chain_id = contract.client().get_chainid().await?;
        // do a quick check to see if we can execute this proposal or not.
        contract
            .execute_proposal_with_signature(
                proposal.data.clone().into(),
                proposal.signature.clone().into(),
            )
            .call()
            .await?;
        // if so, I guess we are good to go here.
        let call = contract.execute_proposal_with_signature(
            proposal.data.clone().into(),
            proposal.signature.clone().into(),
        );
        let data_hash = utils::keccak256(proposal.data.as_slice());
        // check if we already have a execute tx in the queue
        // if we do, we should not create a new one
        let key = SledQueueKey::from_evm_with_custom_key(
            dest_chain_id,
            make_execute_proposal_key(&data_hash),
        );
        let already_queued =
            QueueStore::<TypedTransaction>::has_item(&store, key)?;
        if already_queued {
            tracing::debug!(
                data_hash = ?hex::encode(&data_hash),
                "Skipping this execute for proposal... already in queue",
            );
            return Ok(());
        }
        tracing::debug!(
            data_hash = ?hex::encode(&data_hash),
            sig = ?hex::encode(&proposal.signature),
            "Executing Proposal with signature"
        );
        // enqueue the transaction.
        store.enqueue_item(key, call.tx)?;
        Ok(())
    }
}

fn make_execute_proposal_key(data_hash: &[u8]) -> [u8; 64] {
    let mut result = [0u8; 64];
    result[0..32].copy_from_slice(b"execute_proposal_txx_key_prefix_");
    result[32..64].copy_from_slice(data_hash);
    result
}
