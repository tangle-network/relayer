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
use crate::store::sled::SledStore;
use crate::store::BridgeCommand;

type HttpProvider = providers::Provider<providers::Http>;

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
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
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
                // TODO: handle execute proposal with signature.
            }
        };
        Ok(())
    }
}
