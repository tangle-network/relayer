use std::convert::TryFrom;
use std::marker::PhantomData;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::darkwebb::AnchorContract;
use webb::evm::contract::darkwebb::AnchorContractEvents;
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
pub struct AnchorWatcher<H>(PhantomData<H>);

impl<H> AnchorWatcher<H> {
    pub const fn new() -> AnchorWatcher<H> {
        Self(PhantomData)
    }
}

pub type AnchorLeavesWatcher = AnchorWatcher<ForLeaves>;
pub type AnchorBridgeWatcher = AnchorWatcher<ForBridge>;

#[derive(Clone, Debug)]
pub struct AnchorContractWrapper<M: Middleware> {
    config: config::AnchorContractConfig,
    webb_config: config::WebbRelayerConfig,
    contract: AnchorContract<M>,
}

impl<M: Middleware> AnchorContractWrapper<M> {
    pub fn new(
        config: config::AnchorContractConfig,
        webb_config: config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: AnchorContract::new(config.common.address, client),
            config,
            webb_config,
        }
    }
}

impl<M: Middleware> ops::Deref for AnchorContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> super::WatchableContract for AnchorContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorWatcher<ForLeaves> {
    const TAG: &'static str = "Anchor Watcher For Leaves";

    type Middleware = HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = AnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, log): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use AnchorContractEvents::*;
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
                tracing::debug!(
                    "detected log.block_number: {}",
                    log.block_number
                );
                tracing::debug!(
                    "Saved Deposit Event ({}, {})",
                    value.0,
                    value.1
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

#[async_trait::async_trait]
impl super::EventWatcher for AnchorWatcher<ForBridge> {
    const TAG: &'static str = "Anchor Watcher For Bridge";
    type Middleware = HttpProvider;

    type Contract = AnchorContractWrapper<Self::Middleware>;

    type Events = AnchorContractEvents;

    type Store = SledStore;

    #[tracing::instrument(skip_all)]
    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        (event, _): (Self::Events, LogMeta),
    ) -> anyhow::Result<()> {
        use AnchorContractEvents::*;
        // only process anchor deposit events.
        let event_data = match event {
            DepositFilter(data) => data,
            _ => return Ok(()),
        };
        let client = wrapper.contract.client();
        let origin_chain_id = client.get_chainid().await?;
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
        //      a. dest_contract (the Anchor contract on dest_chain).
        //      b. dest_handler (used for creating data_hash).
        //      c. origin_chain_id (used for creating proposal).
        //      d. leaf_index (used as nonce, for creating proposal).
        //      e. merkle_root (the new merkle_root, used for creating proposal).
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
                AnchorContract::new(linked_anchor.address, dest_client);
            let dest_bridge = dest_contract.bridge().call().await?;
            let dest_handler = dest_contract.handler().call().await?;
            let key = BridgeKey::new(dest_bridge, dest_chain_id);
            let bridge = BridgeRegistry::lookup(key);
            match bridge {
                Some(signal) => {
                    tracing::debug!(
                        "Signaling Bridge@{} to create a new proposal from Anchor@{}",
                        dest_chain_id,
                        origin_chain_id,
                    );
                    signal
                        .send(BridgeCommand::CreateProposal(ProposalData {
                            anchor_address: dest_contract.address(),
                            anchor_handler_address: dest_handler,
                            origin_chain_id,
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
