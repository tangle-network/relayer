use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::tangle_runtime::api::runtime_types::bounded_collections::bounded_vec::BoundedVec;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, ResourceId};
use webb::substrate::subxt::OnlineClient;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::proposal::{Proposal, ProposalKind};
use webb_proposals::ProposalTrait;
use webb_relayer_store::queue::{QueueStore, QueueItem, TransactionQueueItemKey};
use webb_relayer_utils::{metric, TangleRuntimeConfig};
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb_relayer_store::SledStore;
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;

type DkgClient = OnlineClient<TangleRuntimeConfig>;
/// A ProposalSigningBackend that uses the DKG System for Signing Proposals.
#[derive(typed_builder::TypedBuilder)]
pub struct DkgProposalSigningBackend {
    #[builder(setter(into))]
    pub client: DkgClient,
    /// Something that implements the QueueStore trait.
    #[builder(setter(into))]
    store: Arc<SledStore>,
    /// The chain id of the chain that this backend is running on.
    ///
    /// This used as the source chain id for the proposals.
    #[builder(setter(into))]
    src_chain_id: webb_proposals::TypedChainId,
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
            webb_proposals_typed_chain_converter(self.src_chain_id);
        let chain_nonce_addrs = RuntimeApi::storage()
            .dkg_proposals()
            .chain_nonces(&src_chain_id);
        let maybe_whitelisted = self
            .client
            .storage()
            .at_latest()
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
            .at_latest()
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
        let my_chain_id_addr =
            RuntimeApi::constants().dkg_proposals().chain_identifier();
        let my_chain_id = self.client.constants().at(&my_chain_id_addr)?;
        let my_chain_id = match my_chain_id {
            TypedChainId::Substrate(chain_id) => chain_id,
            TypedChainId::PolkadotParachain(chain_id) => chain_id,
            TypedChainId::KusamaParachain(chain_id) => chain_id,
            _ => return Err(webb_relayer_utils::Error::Generic(
                "dkg proposal signing backend only supports substrate chains",
            )),
        };
        let tx_api = RuntimeApi::tx().dkg_proposals();
        let resource_id = proposal.header().resource_id();
        let nonce = proposal.header().nonce();
        tracing::debug!(
            nonce = nonce.0,
            resource_id = hex::encode(resource_id.into_bytes()),
            src_chain_id = ?self.src_chain_id,
            proposal = format!("0x{}", hex::encode(proposal.to_vec())),
            "sending proposal to DKG runtime"
        );

        let unsigned_proposal = Proposal::Unsigned {
            kind: ProposalKind::AnchorUpdate,
            data: BoundedVec(proposal.to_vec()),
        };
        let acknowledge_proposal_tx =
            tx_api.acknowledge_proposal(unsigned_proposal);
        let metadata = self.client.metadata();
        let tx = TypeErasedStaticTxPayload::try_from((
            &metadata,
            acknowledge_proposal_tx,
        ))?;
        let tx_key = SledQueueKey::from_substrate_with_custom_key(
            my_chain_id,
            tx.item_key(),
        );
        // Enqueue transaction in protocol-substrate transaction queue
        let item = QueueItem::new(tx);
        QueueStore::enqueue_item(&self.store, tx_key, item)?;
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
        _ => unimplemented!("Unsupported Chain"),
    }
}
