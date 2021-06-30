use super::handler::*;
use bulletproofs::r1cs::Prover;
use bulletproofs::{BulletproofGens, PedersenGens};
use bulletproofs_gadgets::fixed_deposit_tree::builder::{
    FixedDepositTree, FixedDepositTreeBuilder,
};
use bulletproofs_gadgets::poseidon::builder::Poseidon;
use bulletproofs_gadgets::poseidon::{PoseidonBuilder, PoseidonSbox};
use curve25519_dalek::scalar::Scalar;
use futures::prelude::*;
use merlin::Transcript;
use sp_keyring::AccountKeyring;
use webb::pallet::merkle::*;
use webb::pallet::mixer::*;
use webb::pallet::*;
use webb::substrate::subxt::sp_runtime::AccountId32;
use webb::substrate::subxt::{Client, ClientBuilder, PairSigner};
use webb::substrate::WebbRuntime;

type CachedRoots = CachedRootsStore<WebbRuntime>;
type Leaves = LeavesStore<WebbRuntime>;

// Default hasher instance used to construct the tree
fn default_hasher() -> Poseidon {
    let width = 6;
    // TODO: should be able to pass the number of generators
    let bp_gens = BulletproofGens::new(16400, 1);
    PoseidonBuilder::new(width)
        .bulletproof_gens(bp_gens)
        .sbox(PoseidonSbox::Exponentiation3)
        .build()
}
async fn get_client() -> Client<WebbRuntime> {
    ClientBuilder::new()
        .set_url("ws://127.0.0.1:9944")
        .build()
        .await
        .unwrap()
}

fn generate_proof(
    tree: &mut FixedDepositTree,
    mixer_id: u32,
    cached_block: u32,
    leaf: [u8; 32],
    root: [u8; 32],
    recipient: AccountId32,
    relayer: AccountId32,
) -> SubstrateRelayerWithdrawProof {
    let pc_gens = PedersenGens::default();
    let bp_gens = BulletproofGens::new(16400, 1);
    let mut prover_transcript = Transcript::new(b"zk_membership_proof");
    let prover = Prover::new(&pc_gens, &mut prover_transcript);

    let root = Scalar::from_bytes_mod_order(root);
    let leaf = Scalar::from_bytes_mod_order(leaf);
    let recipient = Scalar::from_bytes_mod_order(*recipient.as_ref());
    let relayer = Scalar::from_bytes_mod_order(*relayer.as_ref());
    let (
        proof_bytes,
        (comms, nullifier_hash, leaf_index_commitments, proof_commitments),
    ) = tree.prove_zk(root, leaf, recipient, relayer, &bp_gens, prover);

    let comms = comms
        .into_iter()
        .map(|v| Commitment(v.to_bytes()))
        .collect();
    let leaf_index_commitments = leaf_index_commitments
        .into_iter()
        .map(|v| Commitment(v.to_bytes()))
        .collect();
    let proof_commitments = proof_commitments
        .into_iter()
        .map(|v| Commitment(v.to_bytes()))
        .collect();
    let nullifier_hash = ScalarData(nullifier_hash.to_bytes());
    let proof_bytes = proof_bytes.to_bytes();
    let recipient = ScalarData(recipient.to_bytes());
    let relayer = ScalarData(relayer.to_bytes());
    let proof: mixer::WithdrawProof<WebbRuntime> = mixer::WithdrawProof {
        relayer: Some(AccountId32::from(relayer.0)),
        recipient: Some(AccountId32::from(recipient.0)),
        proof_bytes,
        nullifier_hash,
        proof_commitments,
        leaf_index_commitments,
        comms,
        mixer_id,
        cached_root: ScalarData(root.to_bytes()),
        cached_block,
    };
    proof.into()
}

#[tokio::test]
async fn relay() {
    let mut tree = FixedDepositTreeBuilder::new()
        .hash_params(default_hasher())
        .depth(32)
        .build();
    let tree_id = 0;
    let leaf = tree.generate_secrets();
    let client = get_client().await;
    let pair = PairSigner::new(AccountKeyring::Alice.pair());
    let result = client
        .deposit_and_watch(&pair, tree_id, vec![ScalarData(leaf.to_bytes())])
        .await;
    let xt = result.unwrap();
    println!("Hash: {:?}", xt.block);
    let maybe_block = client.block(Some(xt.block)).await.unwrap();
    let signed_block = maybe_block.unwrap();
    println!("Number: #{}", signed_block.block.header.number);
    let leaves = {
        let mut leaves = vec![];
        for i in 0..1024 {
            let maybe_leaf = client
                .fetch(&Leaves::try_get(tree_id, i), None)
                .await
                .unwrap();
            match maybe_leaf {
                Some(leaf) if leaf.0 != [0u8; 32] => leaves.push(leaf.0),
                _ => continue,
            };
        }
        leaves
    };
    let cached_roots = client
        .fetch(&CachedRoots::new(signed_block.block.header.number, 0), None)
        .await
        .unwrap();
    let roots = cached_roots.unwrap();
    let root = roots[0];
    tree.tree.add_leaves(leaves, Some(root.0));

    let recipient = AccountKeyring::Charlie.to_account_id();
    let relayer = AccountKeyring::Bob.to_account_id();

    let proof = generate_proof(
        &mut tree,
        tree_id,
        signed_block.block.header.number,
        leaf.to_bytes(),
        root.0,
        recipient,
        relayer,
    );
    let config = crate::config::WebbRelayerConfig {
        port: 9944,
        suri: String::from("//Alice"),
    };
    let ctx = crate::context::RelayerContext::new(config);

    let event_stream = handle_cmd(
        ctx,
        Command::Substrate(SubstrateCommand::Webb(
            SubstrateWebbCommand::RelayWithdrew(proof),
        )),
    );
    futures::pin_mut!(event_stream);

    let event = event_stream.next().await;
    dbg!(&event);
    assert_eq!(event, Some(CommandResponse::Withdraw(WithdrawStatus::Sent)));

    let event = event_stream.next().await;
    dbg!(&event);
    assert_eq!(
        event,
        Some(CommandResponse::Withdraw(WithdrawStatus::Submitted))
    );

    let event = event_stream.next().await;
    dbg!(&event);
    assert_eq!(
        event,
        Some(CommandResponse::Withdraw(WithdrawStatus::Finlized))
    );
}
