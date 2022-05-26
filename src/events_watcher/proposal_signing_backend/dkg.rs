use futures::StreamExt;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::{TypedChainId, ResourceId, Nonce};
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::{dkg_runtime, subxt};
use webb_proposals::evm;
use webb_proposals::substrate;

type DkgConfig = subxt::DefaultConfig;
type DkgRuntimeApi =
    dkg_runtime::api::RuntimeApi<DkgConfig, subxt::DefaultExtra<DkgConfig>>;

/// A ProposalSigningBackend that uses the DKG System for Signing Proposals.
pub struct DkgProposalSigningBackend<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    api: R,
    pair: subxt::PairSigner<C, subxt::DefaultExtra<C>, Sr25519Pair>,
}

impl<R, C> DkgProposalSigningBackend<R, C>
where
    R: From<subxt::Client<C>>,
    C: subxt::Config,
{
    pub fn new(
        client: subxt::Client<C>,
        pair: subxt::PairSigner<C, subxt::DefaultExtra<C>, Sr25519Pair>,
    ) -> Self {
        Self {
            api: client.to_runtime_api(),
            pair,
        }
    }
}

//AnchorUpdateProposal for evm
#[async_trait::async_trait]
impl super::ProposalSigningBackend<evm::AnchorUpdateProposal>
    for DkgProposalSigningBackend<DkgRuntimeApi, DkgConfig>
{
    async fn can_handle_proposal(
        &self,
        proposal: &evm::AnchorUpdateProposal,
    ) -> anyhow::Result<bool> {
        let header = proposal.header();
        let resource_id = header.resource_id();
        let storage_api = self.api.storage().dkg_proposals();
        let src_chain_id =
            webb_proposals_typed_chain_converter(proposal.src_chain());
        let maybe_whitelisted =
            storage_api.chain_nonces(src_chain_id.clone(), None).await?;
        if maybe_whitelisted.is_none() {
            tracing::warn!(?src_chain_id, "chain is not whitelisted");
            return Ok(false);
        }

        let maybe_resource_id = storage_api
            .resources(ResourceId(resource_id.into_bytes()), None)
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
        proposal: &evm::AnchorUpdateProposal,
    ) -> anyhow::Result<()> {
        let tx_api = self.api.tx().dkg_proposals();

        let leaf_index = proposal.latest_leaf_index();
        let resource_id = proposal.header().resource_id();
        let src_chain_id =
            webb_proposals_typed_chain_converter(proposal.src_chain());
        tracing::debug!(
            %leaf_index,
            resource_id = %hex::encode(&resource_id.into_bytes()),
            src_chain_id = ?src_chain_id,
            proposal = %hex::encode(&proposal.to_bytes()),
            "sending proposal to DKG runtime"
        );
        let xt = tx_api.acknowledge_proposal(
            Nonce(leaf_index),
            src_chain_id,
            ResourceId(resource_id.into_bytes()),
            proposal.to_bytes().into(),
        );
        // TODO: here we should have a substrate based tx queue in the background
        // where just send the raw xt bytes and let it handle the work for us.
        // but this here for now.
        let signer = &self.pair;
        let mut progress = xt.sign_and_submit_then_watch(signer).await?;
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
                        Ok(events) => {
                            tracing::debug!(?events, "tx finalized",);
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

//AnchorUpdateProposal for substrate
#[async_trait::async_trait]
impl super::ProposalSigningBackend<substrate::AnchorUpdateProposal>
    for DkgProposalSigningBackend<DkgRuntimeApi, DkgConfig>
{
    async fn can_handle_proposal(
        &self,
        proposal: &substrate::AnchorUpdateProposal,
    ) -> anyhow::Result<bool> {
        let resource_id = proposal.header().resource_id();
        let storage_api = self.api.storage().dkg_proposals();
        let src_chain_id =
            webb_proposals_typed_chain_converter(proposal.src_chain());
        let maybe_whitelisted =
            storage_api.chain_nonces(src_chain_id.clone(), None).await?;
        if maybe_whitelisted.is_none() {
            tracing::warn!(?src_chain_id, "chain is not whitelisted");
            return Ok(false);
        }

        let maybe_resource_id = storage_api
            .resources(ResourceId(resource_id.into_bytes()), None)
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
        proposal: &substrate::AnchorUpdateProposal,
    ) -> anyhow::Result<()> {
        let tx_api = self.api.tx().dkg_proposals();

        let leaf_index = proposal.latest_leaf_index();
        let resource_id = proposal.header().resource_id();
        let src_chain_id =
            webb_proposals_typed_chain_converter(proposal.src_chain());
        tracing::debug!(
            %leaf_index,
            resource_id = %hex::encode(&resource_id.into_bytes()),
            src_chain_id = ?src_chain_id,
            proposal = %hex::encode(&proposal.to_bytes()),
            "sending proposal to DKG runtime"
        );
        let xt = tx_api.acknowledge_proposal(
            Nonce(leaf_index),
            src_chain_id,
            ResourceId(resource_id.into_bytes()),
            proposal.to_bytes().into(),
        );
        // TODO: here we should have a substrate based tx queue in the background
        // where just send the raw xt bytes and let it handle the work for us.
        // but this here for now.
        let signer = &self.pair;
        let mut progress = xt.sign_and_submit_then_watch(signer).await?;
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
                        Ok(events) => {
                            tracing::debug!(?events, "tx finalized",);
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
    }
}
