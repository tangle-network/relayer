use crate::config::PrivateKey;
use crate::store::sled::SledQueueKey;
use crate::store::{BridgeCommand, BridgeKey, QueueStore};
use std::collections::HashMap;
use std::sync::Arc;
use typed_builder::TypedBuilder;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::utils::keccak256;
use webb_proposals::AnchorUpdateProposal;
use webb_proposals::TypedChainId;

#[derive(Debug, Clone)]
pub struct SignatureBridgeMetadata {
    pub chain_id: TypedChainId,
    pub address: Address,
    pub private_key: PrivateKey,
}

/// A ProposalSigningBackend that uses the Governor's private key to sign proposals.
#[derive(TypedBuilder)]
pub struct MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    /// A map between chain id and its signature bridge contract address.
    #[builder(setter(into))]
    signature_bridges: HashMap<TypedChainId, SignatureBridgeMetadata>,
    /// Something that implements the QueueStore trait.
    store: Arc<S>,
}

impl<S> MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    pub fn bridge_metadata(
        &self,
        chain_id: TypedChainId,
    ) -> anyhow::Result<SignatureBridgeMetadata> {
        self.signature_bridges
            .get(&chain_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("no bridge for chain id {:?}", chain_id)
            })
    }
    /// get the signer of the target chain that will be used to sign proposals.
    pub fn signer(
        &self,
        chain_id: TypedChainId,
    ) -> anyhow::Result<LocalWallet> {
        let metadata = self.bridge_metadata(chain_id)?;
        let key = SecretKey::from_bytes(metadata.private_key.as_bytes())?;
        let signer = LocalWallet::from(key)
            .with_chain_id(metadata.chain_id.underlying_chain_id());
        Ok(signer)
    }
}

#[async_trait::async_trait]
impl<S> super::ProposalSigningBackend<AnchorUpdateProposal>
    for MockedProposalSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey> + Send + Sync + 'static,
{
    async fn can_handle_proposal(
        &self,
        proposal: &AnchorUpdateProposal,
    ) -> anyhow::Result<bool> {
        let dest_chain_id = proposal.header().resource_id().typed_chain_id();
        let known_bridge = self.signature_bridges.contains_key(&dest_chain_id);
        Ok(known_bridge)
    }

    async fn handle_proposal(
        &self,
        proposal: &AnchorUpdateProposal,
    ) -> anyhow::Result<()> {
        // the way this one works is that we get the hash of the proposal bytes,
        // the we use the hash to be signed by the signer.
        // Read more here: https://bit.ly/3rqNYTU
        let dest_chain_id = proposal.header().resource_id().typed_chain_id();
        let bridge_metadata = self.bridge_metadata(dest_chain_id)?;
        let signer = self.signer(dest_chain_id)?;
        let proposal_bytes = proposal.to_bytes();
        let hash = keccak256(&proposal_bytes);
        let signature = signer.sign_hash(H256::from(hash), false);
        let bridge_key = BridgeKey::new(
            bridge_metadata.address,
            bridge_metadata.chain_id.underlying_chain_id().into(),
        );
        tracing::debug!(
            %bridge_key,
            ?proposal,
            "Signaling Signature Bridge to execute proposal",
        );
        let signature_bytes = signature.to_vec();
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SigningBackend,
            backend = "Mocked",
            signal_bridge = %bridge_key,
            data = ?hex::encode(&proposal_bytes),
            signature = ?hex::encode(&signature_bytes),
        );
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
