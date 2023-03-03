use futures::StreamExt;
use hex::FromHex;
use webb::substrate::dkg_runtime::api as RuntimeApi;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::ResourceId as DkgResourceId;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId as DkgTypedChainId;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::nonce::Nonce as DkgNonce;
use webb::substrate::subxt::ext::sp_core::sr25519::Pair;
use webb::substrate::subxt::ext::sp_core::Pair as _;
use webb::substrate::subxt::tx::{PairSigner, TxProgress, TxStatus};
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb_proposals::evm::AnchorUpdateProposal;
use webb_proposals::{FunctionSignature, Nonce, ProposalHeader};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};

#[tokio::test]
async fn submit_anchor_update_proposal() {
    let api = OnlineClient::<PolkadotConfig>::new().await.unwrap();

    let resource_id = ResourceId::new(
        TargetSystem::ContractAddress(
            <[u8; 20]>::from_hex("d30c8839c1145609e564b986f667b273ddcb8496")
                .unwrap(),
        ),
        TypedChainId::Evm(5001),
    );
    assert_eq!(
        resource_id.0,
        <[u8; 32]>::from_hex(
            "000000000000d30c8839c1145609e564b986f667b273ddcb8496010000001389"
        )
        .unwrap()
    );

    let src_resource_id = ResourceId::new(
        TargetSystem::ContractAddress(
            <[u8; 20]>::from_hex("e69a847cd5bc0c9480ada0b339d7f0a8cac2b667")
                .unwrap(),
        ),
        TypedChainId::Evm(5002),
    );
    assert_eq!(
        src_resource_id.0,
        <[u8; 32]>::from_hex(
            "000000000000e69a847cd5bc0c9480ada0b339d7f0a8cac2b66701000000138a"
        )
        .unwrap()
    );

    // print resource IDs
    println!("Resource ID: {}", hex::encode(resource_id.0));
    println!("Source Resource ID: {}", hex::encode(src_resource_id.0));

    let sudo_account: PairSigner<PolkadotConfig, Pair> =
        PairSigner::new(Pair::from_string("//Alice", None).unwrap());
    let account_nonce = api
        .rpc()
        .system_account_next_index(dbg!(sudo_account.account_id()))
        .await
        .unwrap();

    let tx_api = RuntimeApi::tx().dkg_proposals();

    println!("register resource 1");
    let register_resource1 =
        tx_api.set_resource(DkgResourceId(resource_id.into_bytes()), vec![]);
    let mut progress = api
        .tx()
        .sign_and_submit_then_watch_default(&register_resource1, &sudo_account)
        .await
        .unwrap();
    watch_events(&mut progress).await;

    println!("register resource 2");
    let register_resource2 = tx_api
        .set_resource(DkgResourceId(src_resource_id.into_bytes()), vec![]);
    let mut progress = api
        .tx()
        .sign_and_submit_then_watch_default(&register_resource2, &sudo_account)
        .await
        .unwrap();
    watch_events(&mut progress).await;

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
            anchor_update_proposal.to_vec(),
        );

        // TODO: gives error "Resource ID provided isn't mapped to anything"
        // So, there is two ways to solve this:
        //
        // 1. Replicate the same code and use dkgProposalHandler.forceSubmitUnsignedProposal call
        //    with sudo.
        // 2. Before running the script loop, you use sudo to register these resources, using
        //    dkg_proposals pallet, there is a method that you can call as sudo to register
        //    these resources, which you will find using PolkadotUI
        //    -> dkgProposals.setResource
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
            _ => assert!(false),
        }
    }
}
