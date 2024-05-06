use crate::SigningRulesContractWrapper;
use std::sync::Arc;
use tokio::sync::Mutex;
use webb::evm::ethers::middleware::SignerMiddleware;
use webb::evm::ethers::providers::Middleware;
use webb_proposals::ProposalTrait;
use webb_relayer_context::RelayerContext;
use webb_relayer_types::EthersClient;
use webb_relayer_utils::metric;

/// A ProposalSigningBackend that uses Signing Rules Contract for Signing Proposals.
#[derive(typed_builder::TypedBuilder)]
pub struct SigningRulesBackend {
    /// Relayer context
    #[builder(setter(into))]
    ctx: RelayerContext,
    /// Signing rules contract
    wrapper: SigningRulesContractWrapper<EthersClient>,
    /// The chain id of the chain that this backend is running on.
    ///
    /// This used as the source chain id for the proposals.
    #[builder(setter(into))]
    src_chain_id: u32,
    /// The tangle chain id where the signing rules contract is deployed.
    /// This is where the proposals will be sent for voting.
    #[builder(setter(into))]
    tangle_chain_id: u32,
}

//AnchorUpdateProposal for evm
#[async_trait::async_trait]
impl super::ProposalSigningBackend for SigningRulesBackend {
    async fn can_handle_proposal(
        &self,
        _proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<bool> {
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
            "Sending proposal for voting on signing rules contract"
        );

        let phase_one_job_id = self.wrapper.config.phase_one_job_id;
        let phase2_job_details = proposal.to_vec();
        let vote_proposal_fn_call = self
            .wrapper
            .contract
            .vote_proposal(phase_one_job_id, phase2_job_details.into());
        let provider = self.ctx.evm_provider(self.tangle_chain_id).await?;
        let wallet = self.ctx.evm_wallet(self.tangle_chain_id).await?;
        let signer_client = SignerMiddleware::new(provider.clone(), wallet);
        let maybe_result = signer_client
            .send_transaction(vote_proposal_fn_call.clone().tx, None)
            .await;

        match maybe_result {
            Ok(tx_hash) => {
                let tx_hash = *tx_hash;
                tracing::info!(
                    tx_hash = tx_hash.to_string(),
                    proposal = format!("0x{}", hex::encode(proposal.to_vec())),
                    src_chain_id = ?self.src_chain_id,
                    "Vote proposal transaction sent successfully"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = ?e,
                    "Error sending vote proposal transaction"
                );
            }
        };

        Ok(())
    }
}
