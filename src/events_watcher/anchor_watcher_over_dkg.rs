use std::convert::TryFrom;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::darkwebb::AnchorContract;
use webb::evm::contract::darkwebb::AnchorContractEvents;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::{dkg_runtime, subxt};

use crate::config;
use crate::events_watcher::{
    create_resource_id, create_update_proposal_data, ProposalData,
};
use crate::store::sled::SledStore;

type HttpProvider = providers::Provider<providers::Http>;

pub struct AnchorWatcherWithSubstrate<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config + subxt::ExtrinsicExtraData<C>,
{
    api: R,
    pair: subxt::PairSigner<C, Sr25519Pair>,
}

impl<R, C> AnchorWatcherWithSubstrate<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config + subxt::ExtrinsicExtraData<C>,
{
    pub fn new(
        client: subxt::Client<C>,
        pair: subxt::PairSigner<C, Sr25519Pair>,
    ) -> Self {
        Self {
            api: client.to_runtime_api(),
            pair,
        }
    }
}

type DKGConfig = dkg_runtime::api::DefaultConfig;
type DKGRuntimeApi = dkg_runtime::api::RuntimeApi<DKGConfig>;
pub type AnchorWatcherOverDKG =
    AnchorWatcherWithSubstrate<DKGRuntimeApi, DKGConfig>;

#[derive(Clone, Debug)]
pub struct AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    config: config::AnchorContractOverDKGConfig,
    webb_config: config::WebbRelayerConfig,
    contract: AnchorContract<M>,
}

impl<M> AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    pub fn new(
        config: config::AnchorContractOverDKGConfig,
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

impl<M> ops::Deref for AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> super::WatchableContract for AnchorContractOverDKGWrapper<M>
where
    M: Middleware,
{
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }
}

#[async_trait::async_trait]
impl super::EventWatcher for AnchorWatcherOverDKG {
    const TAG: &'static str = "Anchor Watcher Over DKG";
    type Middleware = HttpProvider;

    type Contract = AnchorContractOverDKGWrapper<Self::Middleware>;

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
            let dest_handler = dest_contract.handler().call().await?;
            let data = ProposalData {
                anchor_address: dest_contract.address(),
                anchor_handler_address: dest_handler,
                origin_chain_id,
                leaf_index,
                merkle_root: root,
            };
            let update_data = create_update_proposal_data(
                data.origin_chain_id,
                data.leaf_index,
                data.merkle_root,
            );
            let proposal_data = hex::decode(format!(
                "{:x}{}",
                data.anchor_handler_address, update_data
            ))?;
            let resource_id =
                create_resource_id(data.anchor_address, dest_chain_id)?;
            // first we need to do some checks before sending the proposal.
            // 1. check if the origin_chain_id is whitleisted.
            let storage_api = self.api.storage().dkg_proposals();
            let maybe_whitelisted = storage_api
                .chain_nonces(data.origin_chain_id.as_u32(), None)
                .await?;
            if maybe_whitelisted.is_none() {
                // chain is not whitelisted.
                tracing::warn!(
                    "chain {} is not whitelisted, skipping proposal",
                    data.origin_chain_id
                );
                continue;
            }
            // 2. check for the resource id if it exists or not.
            // if not, we need to skip the proposal.
            let maybe_resource_id =
                storage_api.resources(resource_id, None).await?;
            if maybe_resource_id.is_none() {
                // resource id doesn't exist.
                tracing::warn!(
                    "resource id 0x{} doesn't exist, skipping proposal",
                    hex::encode(resource_id),
                );
                continue;
            }
            let tx_api = self.api.tx().dkg_proposals();
            let xt = tx_api.acknowledge_proposal(
                data.leaf_index as _,
                data.origin_chain_id.as_u32(),
                resource_id,
                proposal_data,
            );
            let mut signer = self.pair.clone();
            signer.increment_nonce();
            let result = xt.sign_and_submit_then_watch(&signer).await;
            tracing::debug!("sent proposal to dkg: {:?}", result);
        }
        Ok(())
    }
}
