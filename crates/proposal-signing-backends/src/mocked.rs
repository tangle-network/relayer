use ethereum_types::H256;
use std::collections::HashSet;
use std::sync::Arc;
use typed_builder::TypedBuilder;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::utils::keccak256;
use webb_proposals::{ProposalTrait, ResourceId, TypedChainId};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_store::{BridgeCommand, BridgeKey, QueueStore};
use webb_relayer_types::private_key::PrivateKey;
use webb_relayer_utils::metric;

/// A ProposalSigningBackend that uses the Governor's private key to sign proposals.
#[derive(TypedBuilder)]
pub struct MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    /// A map between chain id and its signature bridge system.
    #[builder(setter(into))]
    signature_bridges: HashSet<ResourceId>,
    /// Something that implements the QueueStore trait.
    store: Arc<S>,
    /// The private key of the governor.
    /// **NOTE**: This must be the same for all signature bridges.
    private_key: PrivateKey,
}

impl<S> MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    fn signer(
        &self,
        chain_id: TypedChainId,
    ) -> webb_relayer_utils::Result<LocalWallet> {
        let key = SecretKey::from_be_bytes(self.private_key.as_bytes())?;
        let signer = LocalWallet::from(key)
            .with_chain_id(chain_id.underlying_chain_id());
        Ok(signer)
    }
}

#[async_trait::async_trait]
impl<S> super::ProposalSigningBackend for MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey> + Send + Sync + 'static,
{
    async fn can_handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> webb_relayer_utils::Result<bool> {
        let resource_id = proposal.header().resource_id();
        let known_bridge = self.signature_bridges.contains(&resource_id);
        Ok(known_bridge)
    }

    async fn handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
        metrics: Arc<metric::Metrics>,
    ) -> webb_relayer_utils::Result<()> {
        // Proposal will be signed by active governor/maintainer.
        // Proposal will be then enqueued for execution with BridgeKey as TypedChainId
        let resource_id = proposal.header().resource_id();
        let dest_chain_id = resource_id.typed_chain_id();
        let signer = self.signer(dest_chain_id)?;
        let proposal_bytes = proposal.to_vec();
        let hash = keccak256(&proposal_bytes);
        let signature = signer.sign_hash(H256::from(hash));
        let bridge_key = BridgeKey::new(dest_chain_id);
        tracing::debug!(
            %bridge_key,
            proposal = ?hex::encode(proposal.to_vec()),
            "Signaling Signature Bridge to execute proposal",
        );
        let signature_bytes = signature.to_vec();
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SigningBackend,
            backend = "Mocked",
            signal_bridge = %bridge_key,
            data = ?hex::encode(&proposal_bytes),
            signature = ?hex::encode(&signature_bytes),
        );
        // Proposal signed metric
        metrics.proposals_signed.inc();
        // now all we have to do is to send the data and the signature to the signature bridge.
        self.store.enqueue_item(
            SledQueueKey::from_bridge_key(bridge_key),
            BridgeCommand::ExecuteProposalWithSignature {
                data: proposal_bytes.to_vec(),
                signature: signature_bytes,
            },
        )?;

        Ok(())
    }
}
