use std::sync::Arc;
use futures::StreamExt;
use tokio::sync::Mutex;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, ResourceId};
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::nonce::Nonce;
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb::substrate::subxt::ext::sp_core::sr25519::Pair as Sr25519Pair;
use webb_proposals::{ProposalTrait};
use webb::substrate::scale::{Encode, Decode};
use webb_relayer_utils::metric;
use webb::substrate::subxt::tx::{PairSigner, TxStatus as TransactionStatus};
use webb::substrate::dkg_runtime::api as RuntimeApi;
type DkgConfig = PolkadotConfig;
type DkgClient = OnlineClient<DkgConfig>;
/// A ProposalSigningBackend that uses the DKG System for Signing Proposals.
pub struct DkgProposalSigningBackend {
    client: DkgClient,
    pair: PairSigner<PolkadotConfig, Sr25519Pair>,
    typed_chain_id: webb_proposals::TypedChainId,
}

impl DkgProposalSigningBackend {
    pub fn new(
        client: OnlineClient<PolkadotConfig>,
        pair: PairSigner<PolkadotConfig, Sr25519Pair>,
        typed_chain_id: webb_proposals::TypedChainId,
    ) -> Self {
        Self {
            client,
            pair,
            typed_chain_id,
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
            .fetch(&chain_nonce_addrs, None)
            .await?;

        if maybe_whitelisted.is_none() {
            tracing::warn!(?src_chain_id, "chain is not whitelisted");
            return Ok(false);
        }
        let resource_id_addrs = RuntimeApi::storage()
            .dkg_proposals()
            .resources(&ResourceId(resource_id.into_bytes()));
        let maybe_resource_id = self
            .client
            .storage()
            .fetch(&resource_id_addrs, None)
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
        metrics: Arc<Mutex<metric::Metrics>>,
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
            src_chain_id = ?src_chain_id,
            proposal = %hex::encode(proposal.to_vec()),
            "sending proposal to DKG runtime"
        );
        let xt = tx_api.acknowledge_proposal(
            nonce,
            src_chain_id,
            ResourceId(resource_id.into_bytes()),
            proposal.to_vec(),
        );

        // TODO: here we should have a substrate based tx queue in the background
        // where just send the raw xt bytes and let it handle the work for us.
        // but this here for now.
        let signer = &self.pair;
        let mut progress = self
            .client
            .tx()
            .sign_and_submit_then_watch_default(&xt, signer)
            .await?;

        while let Some(event) = progress.next().await {
            let e = match event {
                Ok(e) => e,
                Err(err) => {
                    tracing::error!(error = %err, "failed to watch for tx events");
                    return Err(err.into());
                }
            };

            match e {
                TransactionStatus::Future => {}
                TransactionStatus::Ready => {
                    tracing::trace!("tx ready");
                }
                TransactionStatus::Broadcast(_) => {}
                TransactionStatus::InBlock(_) => {
                    tracing::trace!("tx in block");
                }
                TransactionStatus::Retracted(_) => {
                    tracing::warn!("tx retracted");
                }
                TransactionStatus::FinalityTimeout(_) => {
                    tracing::warn!("tx timeout");
                }
                TransactionStatus::Finalized(v) => {
                    let maybe_success = v.wait_for_success().await;
                    match maybe_success {
                        Ok(_events) => {
                            let metrics = metrics.lock().await;
                            metrics.proposals_signed.inc();
                            drop(metrics);
                            tracing::debug!("tx finalized");
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "tx failed");
                            return Err(err.into());
                        }
                    }
                }
                TransactionStatus::Usurped(_) => {}
                TransactionStatus::Dropped => {
                    tracing::warn!("tx dropped");
                }
                TransactionStatus::Invalid => {
                    tracing::warn!("tx invalid");
                }
            }
        }
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
