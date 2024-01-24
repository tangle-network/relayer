use bounded_collections::BoundedVec;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::subxt::OnlineClient;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb_proposals::{Proposal, ProposalKind, ProposalTrait, TypedChainId};
use webb_relayer_store::queue::{
    QueueItem, QueueStore, TransactionQueueItemKey,
};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_store::SledStore;
use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;
use webb_relayer_utils::{metric, TangleRuntimeConfig};

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
    src_chain_id: TypedChainId,
}

//AnchorUpdateProposal for evm
#[async_trait::async_trait]
impl super::ProposalSigningBackend for DkgProposalSigningBackend {
    async fn can_handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<bool> {
        // all is good!
        Ok(true)
    }

    async fn handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let resource_id = proposal.header().resource_id();
        let nonce = proposal.header().nonce();
        tracing::debug!(
            nonce = nonce.0,
            resource_id = hex::encode(resource_id.into_bytes()),
            src_chain_id = ?self.src_chain_id,
            proposal = format!("0x{}", hex::encode(proposal.to_vec())),
            "sending proposal to DKG runtime"
        );

        // let unsigned_proposal = Proposal::Unsigned {
        //     kind: ProposalKind::AnchorUpdate,
        //     data: BoundedVec::try_from(proposal.to_vec()).unwrap()
        // };

        // vote proposal for signing

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
