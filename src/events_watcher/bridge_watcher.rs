use std::collections::HashMap;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use webb::evm::contract::darkwebb::{BridgeContract, BridgeContractEvents};
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::evm::ethers::utils;

use crate::config;
use crate::store::sled::SledStore;

use super::{BridgeWatcher, EventWatcher};

type BridgeConnectionSender = tokio::sync::mpsc::Sender<BridgeCommand>;
type BridgeConnectionReceiver = tokio::sync::mpsc::Receiver<BridgeCommand>;
type Registry = RwLock<HashMap<BridgeKey, BridgeConnectionSender>>;
type HttpProvider = providers::Provider<providers::Http>;

static BRIDGE_REGISTRY: OnceCell<Registry> = OnceCell::new();

/// A BridgeKey is used as a key in the registry.
/// based on the bridge address and the chain id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BridgeKey {
    address: types::Address,
    chain_id: types::U256,
}

impl BridgeKey {
    pub fn new(address: types::Address, chain_id: types::U256) -> Self {
        Self { address, chain_id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProposalData {
    pub dest_contract: types::Address,
    pub dest_handler: types::Address,
    pub origin_chain_id: types::U256,
    pub block_height: types::U64,
    pub leaf_index: u32,
    pub merkle_root: [u8; 32],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BridgeCommand {
    CreateProposal(ProposalData),
}

/// A Bridge Registry is a simple Key-Value store, that provides an easy way to register
/// and discover bridges. For easy communication between the Anchors that connect to that bridge.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BridgeRegistry;

impl BridgeRegistry {
    /// Registers a new Bridge to the registry.
    /// This returns a BridgeConnectionReceiver which will receive commands from whoever
    /// would lookup for this bridge.
    pub fn register(key: BridgeKey) -> BridgeConnectionReceiver {
        let (tx, rx) = tokio::sync::mpsc::channel(50);
        let registry = BRIDGE_REGISTRY.get_or_init(Self::init_registry);
        registry.write().insert(key, tx);
        rx
    }

    /// Unregisters a Bridge from the registry.
    /// this will remove the bridge connection receiver from the registry and close any channels.
    #[allow(dead_code)]
    pub fn unregister(key: BridgeKey) {
        let registry = BRIDGE_REGISTRY.get_or_init(Self::init_registry);
        registry.write().remove(&key);
    }

    /// Lookup a bridge by key.
    /// Returns the BridgeConnectionSender which can be used to send commands to the bridge.
    pub fn lookup(key: BridgeKey) -> Option<BridgeConnectionSender> {
        let registry = BRIDGE_REGISTRY.get_or_init(Self::init_registry);
        registry.read().get(&key).cloned()
    }

    fn init_registry() -> Registry {
        RwLock::new(Default::default())
    }
}

#[derive(Clone, Debug)]
pub struct BridgeContractWrapper<M: Middleware> {
    config: config::BridgeContractConfig,
    contract: BridgeContract<M>,
}

impl<M: Middleware> BridgeContractWrapper<M> {
    pub fn new(config: config::BridgeContractConfig, client: Arc<M>) -> Self {
        Self {
            contract: BridgeContract::new(config.common.address, client),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for BridgeContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract for BridgeContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BridgeContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for BridgeContractWatcher {
    type Middleware = HttpProvider;

    type Contract = BridgeContractWrapper<Self::Middleware>;

    type Events = BridgeContractEvents;

    type Store = SledStore;

    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _contract: &Self::Contract,
        event: Self::Events,
    ) -> anyhow::Result<()> {
        // TODO(@shekohex): Handle the events here.
        tracing::trace!("Got Event {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl BridgeWatcher for BridgeContractWatcher {
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        cmd: BridgeCommand,
    ) -> anyhow::Result<()> {
        use BridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            CreateProposal(data) => {
                self.create_proposal(store, &wrapper.contract, data).await?;
            }
        };
        Ok(())
    }
}

impl BridgeContractWatcher
where
    Self: BridgeWatcher,
{
    #[tracing::instrument(skip(self, _store, contract))]
    async fn create_proposal(
        &self,
        _store: Arc<<Self as EventWatcher>::Store>,
        contract: &BridgeContract<<Self as EventWatcher>::Middleware>,
        data: ProposalData,
    ) -> anyhow::Result<()> {
        let dest_chain_id = contract.client().get_chainid().await?;
        let update_data = create_update_proposal_data(
            data.origin_chain_id,
            data.block_height,
            data.merkle_root,
        );
        tracing::debug!("update_data = {:?}", update_data);
        let pre_hashed = format!("{:?}{}", data.dest_handler, update_data);
        tracing::debug!("data to be hashed = {:?}", pre_hashed);
        let data_hash = utils::keccak256(pre_hashed);
        let resource_id =
            create_resource_id(data.dest_contract, dest_chain_id)?;
        tracing::debug!(
            "Voting with Data = 0x{} & hash = {:?} with resource_id = {:?}",
            update_data,
            data_hash,
            resource_id
        );
        // TODO(@shekohex): we should really check if this proposal is active, before sending Tx.
        // TODO(@shekohex): we need a way to determine whether we should vote
        // now or wait for a bit.
        contract
            .vote_proposal(
                data.origin_chain_id,
                data.leaf_index as _,
                resource_id,
                data_hash,
            )
            .gas(0x5B8D80)
            .call()
            .await?;
        Ok(())
    }
}

fn create_update_proposal_data(
    chain_id: types::U256,
    block_height: types::U64,
    merkle_root: [u8; 32],
) -> String {
    let merkle_root_hex = hex::encode(&merkle_root);
    format!("{:x}{:x}{}", chain_id, block_height, merkle_root_hex,)
}

fn create_resource_id(
    anchor2_address: types::Address,
    chain_id: types::U256,
) -> anyhow::Result<[u8; 32]> {
    let result = format!("{:?}{:04x}", anchor2_address, chain_id);
    tracing::debug!("resource_id(hex): {:?}", result);
    let mut result = result
        .strip_prefix("0x")
        .map(ToOwned::to_owned)
        .context("can't find 0x")?;
    if result.len() % 2 != 0 {
        result.insert(0, '0'); // fix the hex value.
    }
    let hash = hex::decode(&result)?;
    let mut result_bytes = [0u8; 32];
    result_bytes
        .iter_mut()
        .zip(hash.iter().take(32))
        .for_each(|(r, h)| *r = *h);
    Ok(result_bytes)
}
