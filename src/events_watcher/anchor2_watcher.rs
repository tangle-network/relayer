use std::marker::PhantomData;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::darkwebb::Anchor2Contract;
use webb::evm::contract::darkwebb::Anchor2ContractEvents;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;

use crate::config;
use crate::events_watcher::ProposalData;
use crate::events_watcher::{BridgeCommand, BridgeKey, BridgeRegistry};
use crate::store::sled::SledStore;
use crate::store::LeafCacheStore;

pub struct ForLeaves;
pub struct ForBridge;

#[derive(Copy, Clone, Debug)]
pub struct Anchor2Watcher<H>(PhantomData<H>);

impl<H> Anchor2Watcher<H> {
    pub const fn new() -> Anchor2Watcher<H> {
        Self(PhantomData)
    }
}

pub type Anchor2LeavesWatcher = Anchor2Watcher<ForLeaves>;
pub type Anchor2BridgeWatcher = Anchor2Watcher<ForBridge>;

#[derive(Clone, Debug)]
pub struct Anchor2ContractWrapper<M: Middleware> {
    config: config::Anchor2ContractConfig,
    contract: Anchor2Contract<M>,
}

impl<M: Middleware> Anchor2ContractWrapper<M> {
    pub fn new(config: config::Anchor2ContractConfig, client: Arc<M>) -> Self {
        Self {
            contract: Anchor2Contract::new(config.common.address, client),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for Anchor2ContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract for Anchor2ContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for Anchor2Watcher<ForLeaves> {
    type Middleware = providers::Provider<providers::Http>;

    type Contract = Anchor2ContractWrapper<Self::Middleware>;

    type Events = Anchor2ContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip(self, store, event))]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        contract: &Self::Contract,
        event: Self::Events,
    ) -> anyhow::Result<()> {
        match event {
            Anchor2ContractEvents::DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                let value = (leaf_index, H256::from_slice(&commitment));
                store.insert_leaves(contract.address(), &[value])?;
                tracing::trace!(
                    "Saved Deposit Event ({}, {})",
                    value.0,
                    value.1
                );
            }
            _ => {
                tracing::warn!("Unhandled event {:?}", event);
            }
        };

        Ok(())
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for Anchor2Watcher<ForBridge> {
    type Middleware = providers::Provider<providers::Http>;

    type Contract = Anchor2ContractWrapper<Self::Middleware>;

    type Events = Anchor2ContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip(self, _store, e))]
    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        Anchor2ContractWrapper { contract, config }: &Self::Contract,
        e: Self::Events,
    ) -> anyhow::Result<()> {
        use Anchor2ContractEvents::*;
        // only process anchor deposit events.
        let event_data = match e {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
        let bridge_address = contract.bridge().call().await?;
        let client = contract.client();
        let origin_chain_id = contract.chain_id().call().await?;
        let key = BridgeKey::new(bridge_address, origin_chain_id);
        let bridge = BridgeRegistry::lookup(key);
        match bridge {
            Some(signal) => {
                let block_height = client.get_block_number().await?;
                let merkle_root = contract.get_last_root().call().await?;
                let leaf_index = event_data.leaf_index;
                // the correct way for getting the other linked anchors
                // is by getting it from the edge_list, but for now we hardcoded
                // them in the config.
                for linked_anchor in &config.linked_anchors {
                    signal
                        .send(BridgeCommand::CreateProposal(ProposalData {
                            origin_chain_id,
                            dest_chain_id: 0.into(), // FIXME(@shekohex): get the chain id from the config.
                            block_height,
                            merkle_root,
                            leaf_index,
                            origin_contract: contract.address(),
                            dest_contract: linked_anchor.address,
                        }))
                        .await?;
                }
            }
            None => {
                tracing::warn!(
                    "Bridge {} not found in the BridgeRegistry",
                    bridge_address
                );
            }
        }
        Ok(())
    }
}
