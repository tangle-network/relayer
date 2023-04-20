use std::borrow::Cow;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, ResourceId};
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::nonce::Nonce;
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use sp_core::sr25519::Pair as Sr25519Pair;
use webb::evm::ethers::utils;
use webb_proposals::ProposalTrait;
use webb::substrate::scale::{Encode, Decode};
use webb_relayer_utils::metric;
use webb::substrate::dkg_runtime::api as RuntimeApi;
use webb::substrate::subxt::dynamic::Value;
use webb_relayer_store::{QueueStore, SledStore};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_types::dynamic_payload::WebbDynamicTxPayload;
use webb::substrate::subxt::tx::{PairSigner};
use webb::substrate::dkg_runtime::api::runtime_types::sp_core::bounded::bounded_vec::BoundedVec;

type DkgConfig = PolkadotConfig;
type DkgClient = OnlineClient<DkgConfig>;
/// A ProposalSigningBackend that uses the DKG System for Signing Proposals.
pub struct DkgProposalSigningBackend {
    pub client: DkgClient,
    pub pair: PairSigner<PolkadotConfig, Sr25519Pair>,
    pub typed_chain_id: webb_proposals::TypedChainId,
    /// Something that implements the QueueStore trait.
    store: Arc<SledStore>,
}

impl DkgProposalSigningBackend {
    pub fn new(
        client: OnlineClient<PolkadotConfig>,
        pair: PairSigner<PolkadotConfig, Sr25519Pair>,
        typed_chain_id: webb_proposals::TypedChainId,
        store: Arc<SledStore>,
    ) -> Self {
        Self {
            client,
            pair,
            typed_chain_id,
            store,
        }
    }
}

//AnchorUpdateProposal for evm
#[async_trait::async_trait]
impl super::ProposalSigningBackend for DkgProposalSigningBackend {
    async fn can_handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<bool> {
        let header = proposal.header();
        let resource_id = header.resource_id();

        let src_chain_id =
            webb_proposals_typed_chain_converter(self.typed_chain_id);
        let chain_nonce_addrs = RuntimeApi::storage()
            .dkg_proposals()
            .chain_nonces(&src_chain_id);
        let maybe_whitelisted = self
            .client
            .storage()
            .at(None)
            .await?
            .fetch(&chain_nonce_addrs)
            .await?;

        if maybe_whitelisted.is_none() {
            tracing::warn!(?src_chain_id, "chain is not whitelisted");
            return Ok(false);
        }
        let resource_id_addrs = RuntimeApi::storage()
            .dkg_proposals()
            .resources(ResourceId(resource_id.into_bytes()));
        let maybe_resource_id = self
            .client
            .storage()
            .at(None)
            .await?
            .fetch(&resource_id_addrs)
            .await?;
        if maybe_resource_id.is_none() {
            tracing::warn!(
                resource_id = %hex::encode(resource_id.into_bytes()),
                "resource id doesn't exist!",
            );
            return Ok(false);
        }
        // all is good!
        Ok(true)
    }

    async fn handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let tx_api = RuntimeApi::tx().dkg_proposals();
        let resource_id = proposal.header().resource_id();
        let nonce = proposal.header().nonce();
        let src_chain_id =
            webb_proposals_typed_chain_converter(self.typed_chain_id);
        let nonce = Nonce::decode(&mut nonce.encode().as_slice())?;
        tracing::debug!(
            nonce = %hex::encode(nonce.encode()),
            resource_id = %hex::encode(resource_id.into_bytes()),
            src_chain_id = ?self.typed_chain_id,
            proposal = %hex::encode(proposal.to_vec()),
            "sending proposal to DKG runtime"
        );

        let xt = tx_api.acknowledge_proposal(
            nonce,
            src_chain_id,
            ResourceId(resource_id.into_bytes()),
            BoundedVec(proposal.to_vec()),
        );
        // webb dynamic payload
        let execute_proposal_tx = WebbDynamicTxPayload {
            pallet_name: Cow::Borrowed("DKGProposals"),
            call_name: Cow::Borrowed("acknowledge_proposal"),
            fields: vec![
                Value::u128(u128::from(nonce)),
                Value::u128(u128::from(self.typed_chain_id.chain_id())),
                Value::from_bytes(ResourceId(resource_id.into_bytes()).encode()),
                Value::from_bytes(BoundedVec(proposal.to_vec()).encode()),
            ],
        };
        let data_hash = utils::keccak256(xt.call_data().encode());
        let tx_key = SledQueueKey::from_substrate_with_custom_key(
            self.typed_chain_id.underlying_chain_id(),
            make_ acknowledge_proposal_key(data_hash),
        );
        // Enqueue WebbDynamicTxPayload in protocol-substrate transaction queue
        QueueStore::<WebbDynamicTxPayload>::enqueue_item(
            &self.store,
            tx_key,
            execute_proposal_tx,
        )?;

        Ok(())
    }
}

fn webb_proposals_typed_chain_converter(
    v: webb_proposals::TypedChainId,
) -> TypedChainId {
    match v {
        webb_proposals::TypedChainId::None => TypedChainId::None,
        webb_proposals::TypedChainId::Evm(id) => TypedChainId::Evm(id),
        webb_proposals::TypedChainId::Substrate(id) => {
            TypedChainId::Substrate(id)
        }
        webb_proposals::TypedChainId::PolkadotParachain(id) => {
            TypedChainId::PolkadotParachain(id)
        }
        webb_proposals::TypedChainId::KusamaParachain(id) => {
            TypedChainId::KusamaParachain(id)
        }
        webb_proposals::TypedChainId::RococoParachain(id) => {
            TypedChainId::RococoParachain(id)
        }
        webb_proposals::TypedChainId::Cosmos(id) => TypedChainId::Cosmos(id),
        webb_proposals::TypedChainId::Solana(id) => TypedChainId::Solana(id),
        webb_proposals::TypedChainId::Ink(id) => TypedChainId::Ink(id),
    }
}

pub fn make_ acknowledge_proposal_key(data_hash: [u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    // make sure that this byte array is 32 bytes.
    let prefix = b"acknowledge_proposal_fixed_key_";
    result[0..32].copy_from_slice(prefix);
    result[32..64].copy_from_slice(&data_hash);
    result
}
