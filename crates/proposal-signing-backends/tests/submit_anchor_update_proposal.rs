use futures::StreamExt;
use hex::FromHex;
use webb::substrate::dkg_runtime::api as RuntimeApi;
use webb::substrate::subxt::ext::sp_core::sr25519::Pair;
use webb::substrate::subxt::ext::sp_core::Pair as _;
use webb::substrate::subxt::tx::{PairSigner, TxStatus};
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

        let tx_api = RuntimeApi::tx().dkg_proposals();
        let xt = tx_api.acknowledge_proposal(
            webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::nonce::Nonce(account_nonce),
            webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId::Evm(5001),
            webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::ResourceId(resource_id.into_bytes()),
            anchor_update_proposal.to_vec(),
        );
        let mut progress = api
            .tx()
            .sign_and_submit_then_watch_default(&xt, &sudo_account)
            .await
            .unwrap();
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
                _ => assert!(false)
            }
        }
    }
}
