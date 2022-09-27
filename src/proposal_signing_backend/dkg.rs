use futures::StreamExt;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, ResourceId};
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::nonce::Nonce;
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::{dkg_runtime, subxt};
use webb_proposals::{ProposalTrait};
use webb::substrate::scale::{Encode, Decode};

type DkgConfig = subxt::DefaultConfig;
type DkgRuntimeApi = dkg_runtime::api::RuntimeApi<
    DkgConfig,
    subxt::PolkadotExtrinsicParams<DkgConfig>,
>;
/// A ProposalSigningBackend that uses the DKG System for Signing Proposals.
pub struct DkgProposalSigningBackend<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    api: R,
    pair: subxt::PairSigner<C, Sr25519Pair>,
    typed_chain_id: webb_proposals::TypedChainId,
}

impl<R, C> DkgProposalSigningBackend<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    pub fn new(
        client: subxt::Client<C>,
        pair: subxt::PairSigner<C, Sr25519Pair>,
        typed_chain_id: webb_proposals::TypedChainId,
    ) -> Self {
        Self {
            api: client.to_runtime_api(),
            pair,
            typed_chain_id,
        }
    }
}

//AnchorUpdateProposal for evm
#[async_trait::async_trait]
impl super::ProposalSigningBackend
    for DkgProposalSigningBackend<DkgRuntimeApi, DkgConfig>
{
    async fn can_handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> crate::Result<bool> {
        let header = proposal.header();
        let resource_id = header.resource_id();
        let storage_api = self.api.storage().dkg_proposals();
        let src_chain_id =
            webb_proposals_typed_chain_converter(self.typed_chain_id);
        let maybe_whitelisted =
            storage_api.chain_nonces(&src_chain_id, None).await?;
        if maybe_whitelisted.is_none() {
            tracing::warn!(?src_chain_id, "chain is not whitelisted");
            return Ok(false);
        }

        let maybe_resource_id = storage_api
            .resources(&ResourceId(resource_id.into_bytes()), None)
            .await?;
        if maybe_resource_id.is_none() {
            tracing::warn!(
                resource_id = %hex::encode(&resource_id.into_bytes()),
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
    ) -> crate::Result<()> {
        // Register metric for when handle proposal is being called

        let tx_api = self.api.tx().dkg_proposals();
        let resource_id = proposal.header().resource_id();
        let nonce = proposal.header().nonce();
        let src_chain_id =
            webb_proposals_typed_chain_converter(self.typed_chain_id);
        let nonce = Nonce::decode(&mut nonce.encode().as_slice())?;
        tracing::debug!(
            nonce = %hex::encode(&nonce.encode()),
            resource_id = %hex::encode(&resource_id.into_bytes()),
            src_chain_id = ?src_chain_id,
            proposal = %hex::encode(&proposal.to_vec()),
            "sending proposal to DKG runtime"
        );
        let xt = tx_api.acknowledge_proposal(
            nonce,
            src_chain_id,
            ResourceId(resource_id.into_bytes()),
            proposal.to_vec(),
        )?;
        // TODO: here we should have a substrate based tx queue in the background
        // where just send the raw xt bytes and let it handle the work for us.
        // but this here for now.
        let signer = &self.pair;
        let mut progress =
            xt.sign_and_submit_then_watch_default(signer).await?;
        while let Some(event) = progress.next().await {
            let e = match event {
                Ok(e) => e,
                Err(err) => {
                    tracing::error!(error = %err, "failed to watch for tx events");
                    return Err(err.into());
                }
            };

            match e {
                subxt::TransactionStatus::Future => {}
                subxt::TransactionStatus::Ready => {
                    tracing::trace!("tx ready");
                }
                subxt::TransactionStatus::Broadcast(_) => {}
                subxt::TransactionStatus::InBlock(_) => {
                    tracing::trace!("tx in block");
                }
                subxt::TransactionStatus::Retracted(_) => {
                    tracing::warn!("tx retracted");
                }
                subxt::TransactionStatus::FinalityTimeout(_) => {
                    tracing::warn!("tx timeout");
                }
                subxt::TransactionStatus::Finalized(v) => {
                    let maybe_success = v.wait_for_success().await;
                    match maybe_success {
                        Ok(_events) => {
                            tracing::debug!("tx finalized");
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "tx failed");
                            return Err(err.into());
                        }
                    }
                }
                subxt::TransactionStatus::Usurped(_) => {}
                subxt::TransactionStatus::Dropped => {
                    tracing::warn!("tx dropped");
                }
                subxt::TransactionStatus::Invalid => {
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
