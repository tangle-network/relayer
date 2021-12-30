use std::convert::TryFrom;
use std::ops;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::contract::protocol_solidity::AnchorContract;
use webb::evm::contract::protocol_solidity::AnchorContractEvents;
use webb::evm::ethers::prelude::{Contract, LogMeta, Middleware};
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::{dkg_runtime, scale, subxt};

use crate::config;
use crate::events_watcher::{create_resource_id, ProposalData};
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

    fn max_events_per_step(&self) -> types::U64 {
        self.config.events_watcher.max_events_per_step.into()
    }

    fn print_progress_interval(&self) -> Duration {
        Duration::from_millis(
            self.config.events_watcher.print_progress_interval,
        )
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
            let mut proposal_data = Vec::with_capacity(80);
            let resource_id =
                create_resource_id(data.anchor_address, dest_chain_id)?;
            tracing::trace!("r_id: 0x{}", hex::encode(&resource_id));
            let header = ProposalHeader {
                resource_id,
                chain_id: dest_chain_id.as_u32(),
                function_sig: [0x00, 0x00, 0x00, 0x00], // TODO(@shekohex): get this from the contract.
                nonce: ProposalNonce::from(leaf_index),
            };
            // first the header (40 bytes)
            scale::Encode::encode_to(&header, &mut proposal_data);
            // next, the origin chain id (4 bytes)
            proposal_data.extend_from_slice(
                &data.origin_chain_id.as_u32().to_le_bytes(),
            );
            // next, the leaf index (4 bytes)
            proposal_data.extend_from_slice(&data.leaf_index.to_le_bytes());
            // next, the merkle root (32 bytes)
            proposal_data.extend_from_slice(&data.merkle_root);
            // sanity check
            assert_eq!(proposal_data.len(), 80);
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
            let mut progress = xt.sign_and_submit_then_watch(&signer).await?;
            while let Some(event) = progress.next().await? {
                tracing::debug!("Tx Progress: {:?}", event);
            }
        }
        Ok(())
    }
}

pub type ResourceId = [u8; 32];
pub type ProposalNonce = u64;

#[derive(Debug, Clone, Copy)]
pub struct ProposalHeader {
    pub resource_id: ResourceId,
    pub chain_id: u32,
    pub function_sig: [u8; 4],
    pub nonce: ProposalNonce,
}

impl scale::Encode for ProposalHeader {
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // resource_id contains the chain id already.
        buf.extend_from_slice(&self.resource_id); // 32 bytes
        buf.extend_from_slice(&self.function_sig); // 4 bytes
        buf.extend_from_slice(&(self.nonce as u32).to_le_bytes()); // 4 bytes
        buf
    }

    fn encoded_size(&self) -> usize {
        40 // Bytes
    }
}

impl scale::Decode for ProposalHeader {
    fn decode<I: scale::Input>(input: &mut I) -> Result<Self, scale::Error> {
        let mut data = [0u8; 40];
        input.read(&mut data).map_err(|_| {
            scale::Error::from(
                "input bytes are less than the header size (40 bytes)",
            )
        })?;
        // _NOTE_: rustc won't generate bounds check for the following slice
        // since we know the length of the slice is at least 40 bytes already.

        // decode the resourceId is the first 32 bytes
        let mut resource_id = [0u8; 32];
        resource_id.copy_from_slice(&data[0..32]);
        // the chain id is the last 4 bytes of the **resourceId**
        let mut chain_id_bytes = [0u8; 4];
        chain_id_bytes.copy_from_slice(&resource_id[28..32]);
        let chain_id = u32::from_le_bytes(chain_id_bytes);
        // the function signature is the next first 4 bytes after the resourceId.
        let mut function_sig = [0u8; 4];
        function_sig.copy_from_slice(&data[32..36]);
        // the nonce is the last 4 bytes of the header (also considered as the first arg).
        let mut nonce_bytes = [0u8; 4];
        nonce_bytes.copy_from_slice(&data[36..40]);
        let nonce = u32::from_le_bytes(nonce_bytes);
        let header = ProposalHeader {
            resource_id,
            chain_id,
            function_sig,
            nonce: ProposalNonce::from(nonce),
        };
        Ok(header)
    }
}
