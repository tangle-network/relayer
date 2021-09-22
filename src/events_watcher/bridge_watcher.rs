use std::collections::HashMap;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

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
    type Middleware = SignerMiddleware<HttpProvider, LocalWallet>;

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
        let pre_hashed = format!("{:x}{}", data.dest_handler, update_data);
        let data_to_be_hashed = hex::decode(pre_hashed)?;
        let data_hash = utils::keccak256(data_to_be_hashed);
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
    let chain_id_hex = pad32(&format!("{:x}", chain_id));
    let block_height_hex = pad32(&format!("{:x}", block_height));
    let merkle_root_hex = hex::encode(&merkle_root);
    format!("{}{}{}", chain_id_hex, block_height_hex, merkle_root_hex,)
}

fn create_resource_id(
    anchor2_address: types::Address,
    chain_id: types::U256,
) -> anyhow::Result<[u8; 32]> {
    let chain_id_hex = pad(&format!("{:x}", chain_id), 2);
    let result = format!("{:x}{}", anchor2_address, chain_id_hex);
    let hash = hex::decode(pad32(&result))?;
    let mut result_bytes = [0u8; 32];
    result_bytes
        .iter_mut()
        .zip(hash.iter().take(32))
        .for_each(|(r, h)| *r = *h);
    Ok(result_bytes)
}

fn pad32(val: &str) -> String {
    pad(val, 64)
}

fn pad(val: &str, n: usize) -> String {
    format!("{}{}", "0".repeat(n.saturating_sub(val.len())), val)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn should_create_update_proposal() {
        let chain_id = types::U256::from(4);
        let block_height = types::U64::one();
        let merkle_root = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18,
            19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        ];
        let result =
            create_update_proposal_data(chain_id, block_height, merkle_root);
        let expected = include_str!("../../tests/fixtures/proposal_data.txt")
            .trim_end_matches('\n');
        assert_eq!(result, expected);
        let dest_handler = types::Address::from_str(
            "0x7Bb1Af8D06495E85DDC1e0c49111C9E0Ab50266E",
        )
        .unwrap();
        let pre_hashed = format!("{:x}{}", dest_handler, result);
        let data_to_be_hashed = hex::decode(pre_hashed).unwrap();
        let data_hash = hex::encode(utils::keccak256(data_to_be_hashed));
        let expected_data_hash =
            "45822e043e5735fc2485e52dd71403d140f3a755cd59dc02539eaef3bcfd4bcb";
        assert_eq!(data_hash, expected_data_hash);
    }

    #[test]
    fn should_create_resouce_id() {
        let chain_id = types::U256::from(4);
        let anchor2_address = types::Address::from_str(
            "0xB42139fFcEF02dC85db12aC9416a19A12381167D",
        )
        .unwrap();
        let resource_id =
            create_resource_id(anchor2_address, chain_id).unwrap();
        let expected = hex::decode(
            "0000000000000000000000b42139ffcef02dc85db12ac9416a19a12381167d04",
        )
        .unwrap();
        assert_eq!(resource_id, expected.as_slice());
    }
}
