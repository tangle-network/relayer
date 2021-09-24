use std::convert::TryFrom;
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

type HttpProvider = providers::Provider<providers::Http>;

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
    webb_config: config::WebbRelayerConfig,
    contract: Anchor2Contract<M>,
}

impl<M: Middleware> Anchor2ContractWrapper<M> {
    pub fn new(
        config: config::Anchor2ContractConfig,
        webb_config: config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: Anchor2Contract::new(config.common.address, client),
            config,
            webb_config,
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
    type Middleware = HttpProvider;

    type Contract = Anchor2ContractWrapper<Self::Middleware>;

    type Events = Anchor2ContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip(self, store, wrapper, event))]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        match event {
            Anchor2ContractEvents::DepositFilter(deposit) => {
                let commitment = deposit.commitment;
                let leaf_index = deposit.leaf_index;
                let value = (leaf_index, H256::from_slice(&commitment));
                store.insert_leaves(wrapper.contract.address(), &[value])?;
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
    type Middleware = HttpProvider;

    type Contract = Anchor2ContractWrapper<Self::Middleware>;

    type Events = Anchor2ContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip(self, _store, wrapper))]
    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use Anchor2ContractEvents::*;
        // only process anchor deposit events.
        let event_data = match e {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
        let client = wrapper.contract.client();
        let origin_chain_id = client.get_chainid().await?;
        let block_height = client.get_block_number().await?;
        let root = wrapper.contract.get_last_root().call().await?;
        let leaf_index = event_data.leaf_index;
        // the correct way for getting the other linked anchors
        // is by getting it from the edge_list, but for now we hardcoded
        // them in the config.

        // **The Signaling Flow**
        //
        // For Every Linked Anchor, we do the following:
        // 1. Get the chain information of that anchor from the config,
        //    if not found, we skip (we should print a warning here).
        // 2. We call that chain `dest_chain`, then we create a connection to that
        //    dest_chain, which we will construct the other linked anchor contract
        //    to query the following information:
        //      a. dest_chain_id (the chain_id of that linked anchor).
        //      b. dest_bridge (the bridge of that linked anchor on the other chain).
        //      c. dest_handler (the address of the handler that linked to that anchor).
        // 3. Then we create a `BridgeKey` of that `dest_bridge` to send the proposal data.
        //    if not found, we skip.
        // 4. Signal the bridge with the following data:
        //      a. dest_contract (the anchor2 contract on dest_chain).
        //      b. dest_handler (used for creating data_hash).
        //      c. origin_chain_id (used for creating proposal).
        //      d. block_height (used for creating proposal).
        //      e. leaf_index (used as nonce, for creating proposal).
        //      f. merkle_root (the new merkle_root, used for creating proposal).
        //
        for linked_anchor in &wrapper.config.linked_anchors {
            let dest_chain = linked_anchor.chain.to_lowercase();
            let maybe_chain = wrapper.webb_config.evm.get(&dest_chain);
            let dest_chain = match maybe_chain {
                Some(chain) => chain,
                None => continue,
            };
            // TODO(@shekohex): store clients in connection pool, so don't
            // have to create a new connection every time.
            let provider =
                HttpProvider::try_from(dest_chain.http_endpoint.as_str())?
                    .interval(Duration::from_millis(6u64));
            let dest_client = Arc::new(provider);
            let dest_chain_id = dest_client.get_chainid().await?;
            let dest_contract =
                Anchor2Contract::new(linked_anchor.address, dest_client);
            let contract_chain_id = dest_contract.chain_id().call().await?;
            // sanity check.
            assert_eq!(dest_chain_id, contract_chain_id);
            let dest_bridge = dest_contract.bridge().call().await?;
            let dest_handler = dest_contract.handler().call().await?;
            let key = BridgeKey::new(dest_bridge, dest_chain_id);
            let bridge = BridgeRegistry::lookup(key);
            match bridge {
                Some(signal) => {
                    signal
                        .send(BridgeCommand::CreateProposal(ProposalData {
                            anchor2_address: dest_contract.address(),
                            anchor2_handler_address: dest_handler,
                            origin_chain_id,
                            block_height,
                            leaf_index,
                            merkle_root: root,
                        }))
                        .await?;
                }
                None => {
                    tracing::warn!(
                        "Bridge {} not found in the BridgeRegistry",
                        dest_bridge
                    );
                }
            }
        }
        Ok(())
    }
}
