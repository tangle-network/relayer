use futures::StreamExt;
use sp_core::sr25519::Pair;
use sp_core::Pair as _;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::header::ResourceId as DkgResourceId;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::header::TypedChainId as DkgTypedChainId;
use webb::substrate::tangle_runtime::api::runtime_types::webb_proposals::nonce::Nonce as DkgNonce;
use webb::substrate::tangle_runtime::api::runtime_types::sp_core::bounded::bounded_vec::BoundedVec;
use webb::substrate::subxt::tx::{PairSigner, TxProgress, TxStatus};
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb_bridge_registry_backends::dkg::DkgBridgeRegistryBackend;
use webb_bridge_registry_backends::BridgeRegistryBackend;
use webb_proposals::evm::AnchorUpdateProposal;
use webb_proposals::ResourceId;
use webb_proposals::{FunctionSignature, Nonce, ProposalHeader};

/// Rust reimplementation of DKG submit proposal test
///
/// <https://github.com/webb-tools/dkg-substrate/blob/master/dkg-test-suite/scripts/submitProposals.ts>
#[tokio::test]
#[ignore = "requires running dkg-substrate standalone node"]
async fn submit_anchor_update_proposal() {
    let api = OnlineClient::<PolkadotConfig>::new().await.unwrap();
    let bridge_registry = DkgBridgeRegistryBackend::new(api.clone());

    // retrieve resource ids from bridge registry
    let next_bridge_index = bridge_registry.next_bridge_index().await.unwrap();
    let bridges = bridge_registry
        .bridges(next_bridge_index - 1)
        .await
        .unwrap()
        .unwrap();
    let resource_id = ResourceId(bridges.resource_ids.0[1].0);
    let src_resource_id = ResourceId(bridges.resource_ids.0[0].0);

    // print resource IDs
    println!("Resource ID: {}", hex::encode(resource_id.0));
    println!("Source Resource ID: {}", hex::encode(src_resource_id.0));

    let tx_api = RuntimeApi::tx().dkg_proposals();
    let sudo_account: PairSigner<PolkadotConfig, Pair> =
        PairSigner::new(Pair::from_string("//Alice", None).unwrap());
    let account_nonce = api
        .rpc()
        .system_account_next_index(sudo_account.account_id())
        .await
        .unwrap();

    // following code runs in a loop in original code
    {
        let proposal_header = ProposalHeader::new(
            resource_id,
            FunctionSignature([0, 0, 0, 0]),
            Nonce(account_nonce),
        );
        let anchor_update_proposal = AnchorUpdateProposal::new(
            proposal_header,
            Default::default(),
            src_resource_id,
        );
        let anchor_update_proposal = anchor_update_proposal.to_bytes();
        println!(
            "anchor update proposal bytes: {}",
            hex::encode(anchor_update_proposal)
        );
        assert_eq!(104, anchor_update_proposal.len());

        let xt = tx_api.acknowledge_proposal(
            DkgNonce(account_nonce),
            DkgTypedChainId::Evm(5001),
            DkgResourceId(resource_id.into_bytes()),
            BoundedVec(anchor_update_proposal.to_vec()),
        );

        let mut progress = api
            .tx()
            .sign_and_submit_then_watch_default(&xt, &sudo_account)
            .await
            .unwrap();
        watch_events(&mut progress).await
    }
}

async fn watch_events(
    progress: &mut TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
) {
    while let Some(event) = progress.next().await {
        let e = match event {
            Ok(e) => e,
            Err(err) => {
                println!("failed to watch for tx events: {err}");
                return;
            }
        };

        match e {
            TxStatus::Ready => {
                println!("tx ready");
            }
            TxStatus::Broadcast(_) => {
                println!("tx broadcast");
            }
            TxStatus::InBlock(_) => {
                println!("tx in block");
            }
            TxStatus::Finalized(v) => {
                let maybe_success = v.wait_for_success().await;
                match &maybe_success {
                    Ok(_events) => {
                        println!("tx finalized");
                    }
                    Err(err) => {
                        println!("tx failed: {err}");
                    }
                }
                assert!(maybe_success.is_ok());
            }
            _ => unreachable!(),
        }
    }
}
