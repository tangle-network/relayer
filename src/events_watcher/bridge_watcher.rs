use std::collections::HashMap;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use webb::evm::contract::darkwebb::{
    Anchor2ContractEvents, BridgeContract, BridgeContractEvents,
};
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;

use crate::config;
use crate::store::sled::SledLeafCache;

use super::{BridgeWatcher, EventWatcher};

type BridgeConnectionSender = tokio::sync::mpsc::Sender<BridgeCommand>;
type BridgeConnectionReceiver = tokio::sync::mpsc::Receiver<BridgeCommand>;
type Registry = RwLock<HashMap<BridgeKey, BridgeConnectionSender>>;

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
pub enum BridgeCommand {
    ProcessAnchor2ContractEvents(Anchor2ContractEvents),
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
        // FIXME(@shekohex): should probably handled from the config.
        Duration::from_millis(6_000)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BridgeContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for BridgeContractWatcher {
    type Middleware = providers::Provider<providers::Http>;

    type Contract = BridgeContractWrapper<Self::Middleware>;

    type Events = BridgeContractEvents;

    type Store = SledLeafCache;

    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _contract: &Self::Contract,
        event: Self::Events,
    ) -> anyhow::Result<()> {
        // TODO(@shekohex): Handle the events here.
        tracing::debug!("Got Event {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl BridgeWatcher for BridgeContractWatcher {
    async fn handle_cmd(
        &self,
        _store: Arc<Self::Store>,
        cmd: BridgeCommand,
    ) -> anyhow::Result<()> {
        // TODO(@shekohex): Handle the cmd here.
        tracing::debug!("Got cmd {:?}", cmd);
        Ok(())
    }
}
