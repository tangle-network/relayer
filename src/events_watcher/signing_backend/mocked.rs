use crate::config::PrivateKey;
use crate::store::sled::SledQueueKey;
use crate::store::{BridgeCommand, BridgeKey, QueueStore};
use std::sync::Arc;
use typed_builder::TypedBuilder;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::utils::keccak256;
use webb_proposals::AnchorUpdateProposal;

#[derive(TypedBuilder)]
pub struct MockedSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    private_key: PrivateKey,
    #[builder(setter(into))]
    chain_id: u64,
    #[builder(setter(into))]
    signature_bridge_address: Address,
    store: Arc<S>,
}

impl<S> MockedSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey>,
{
    pub fn signer(&self) -> anyhow::Result<LocalWallet> {
        let key = SecretKey::from_bytes(self.private_key.as_bytes())?;
        Ok(LocalWallet::from(key).with_chain_id(self.chain_id))
    }
}

#[async_trait::async_trait]
impl<S> super::SigningBackend<AnchorUpdateProposal> for MockedSigningBackend<S>
where
    S: QueueStore<BridgeCommand, Key = SledQueueKey> + Send + Sync + 'static,
{
    async fn can_handle_proposal(
        &self,
        _proposal: &AnchorUpdateProposal,
    ) -> anyhow::Result<bool> {
        // we are always ready, sir 🗿
        Ok(true)
    }

    async fn handle_proposal(
        &self,
        proposal: &AnchorUpdateProposal,
    ) -> anyhow::Result<()> {
        let signer = self.signer()?;
        let proposal_bytes = proposal.to_bytes();
        let hash = keccak256(&proposal_bytes);
        let signature = signer.sign_hash(H256::from(hash), false);
        let bridge_key =
            BridgeKey::new(self.signature_bridge_address, self.chain_id.into());
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
