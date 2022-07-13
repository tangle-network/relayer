use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use ethereum_types::U256;
use crate::context::RelayerContext;
use webb::substrate::{
    subxt::{self, PairSigner},
};


pub async fn make_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: U256,
    linked_anchors: Option<Vec<LinkedAnchorConfig>>,
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> anyhow::Result<ProposalSigningBackendSelector> {
    // Check if contract is configured with governance support for the relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!("Governance relaying is not enabled for relayer");
        return Ok(ProposalSigningBackendSelector::None);
    }
    // We do this by checking if linked anchors are provided.
    let linked_anchors = match linked_anchors {
        Some(anchors) => {
            if anchors.is_empty() {
                tracing::warn!("Misconfigured Network: Linked anchors cannot be empty for governance relaying");
                return Ok(ProposalSigningBackendSelector::None);
            } else {
                anchors
            }
        }
        None => {
            tracing::warn!("Misconfigured Network: Linked anchors must be configured for governance relaying");
            return Ok(ProposalSigningBackendSelector::None);
        }
    };

    // we need to check/match on the proposal signing backend configured for this anchor.
    match proposal_signing_backend {
        Some(ProposalSigningBackendConfig::DkgNode(c)) => {
            // if it is the dkg backend, we will need to connect to that node first,
            // and then use the DkgProposalSigningBackend to sign the proposal.
            let typed_chain_id =
                webb_proposals::TypedChainId::Evm(chain_id.as_u32());
            let dkg_client = ctx
                .substrate_provider::<subxt::DefaultConfig>(&c.node)
                .await?;
            let pair = ctx.substrate_wallet(&c.node).await?;
            let backend = DkgProposalSigningBackend::new(
                dkg_client,
                PairSigner::new(pair),
                typed_chain_id,
            );
            Ok(ProposalSigningBackendSelector::Dkg(backend))
        }
        Some(ProposalSigningBackendConfig::Mocked(mocked)) => {
            // if it is the mocked backend, we will use the MockedProposalSigningBackend to sign the proposal.
            // which is a bit simpler than the DkgProposalSigningBackend.
            // get only the linked chains to that anchor.
            let linked_chains = linked_anchors
                .iter()
                .flat_map(|c| ctx.config.evm.get(&c.chain));
            // then will have to go through our configruation to retrieve the correct
            // signature bridges that are configrued on the linked chains.
            // Note: this assumes that every network will only have one signature bridge configured for it.
            let signature_bridges = linked_chains
                .flat_map(|chain_config| {
                    // find the first signature bridge configured on that chain.
                    chain_config
                        .contracts
                        .iter()
                        .find(|contract| {
                            matches!(contract, Contract::SignatureBridge(_))
                        })
                        .map(|contract| (contract, chain_config.chain_id))
                })
                .map(|(bridge_contract, chain_id)| match bridge_contract {
                    Contract::SignatureBridge(c) => (c, chain_id),
                    _ => unreachable!(),
                })
                .flat_map(|(bridge_config, chain_id)| {
                    // then we just create the signature bridge metadata.
                    let chain_id =
                        webb_proposals::TypedChainId::Evm(chain_id as u32);
                    let target_system =
                        webb_proposals::TargetSystem::new_contract_address(
                            bridge_config.common.address,
                        );
                    Some((chain_id, target_system))
                })
                .collect::<HashMap<_, _>>();
            let backend = MockedProposalSigningBackend::builder()
                .store(store.clone())
                .private_key(mocked.private_key)
                .signature_bridges(signature_bridges)
                .build();
            Ok(ProposalSigningBackendSelector::Mocked(backend))
        }
        None => {
            tracing::warn!("Misconfigured Network: Proposal signing backend must be configured for governance relaying");
            Ok(ProposalSigningBackendSelector::None)
        }
    }
}

