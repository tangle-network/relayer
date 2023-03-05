use ark_bn254::Fr as Bn254Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_std::collections::BTreeMap;
use arkworks_native_gadgets::{
    merkle_tree::SparseMerkleTree, poseidon::Poseidon,
};
use arkworks_setups::{common::setup_params, Curve};
use arkworks_utils::{bytes_vec_to_f, parse_vec};

// merkle tree test
#[test]
fn test_merkle_root() {
    let relayer_leaves = vec![
        "0x017dc570cb5c6807dbaa475c9d4e445ac95a73400692541c367786c009c844cf",
        "0x04568790fcfc67d855dfb60de6844f6d82f4b8dc6dd0115f9f04ece21ebffb8d",
    ];
    let params = setup_params::<Bn254Fr>(Curve::Bn254, 5, 3);
    let poseidon = Poseidon::<Bn254Fr>::new(params);
    let leaves: Vec<Bn254Fr> =
        bytes_vec_to_f(&parse_vec(relayer_leaves).unwrap());
    let pairs: BTreeMap<u32, Bn254Fr> = leaves
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u32, *l))
        .collect();
    type Smt = SparseMerkleTree<Bn254Fr, Poseidon<Bn254Fr>, 30>;
    let default_leaf_hex = vec![
        "0x2fe54c60d3acabf3343a35b6eba15db4821b340f76e741e2249685ed4899af6c",
    ];
    let default_leaf_scalar: Vec<Bn254Fr> =
        bytes_vec_to_f(&parse_vec(default_leaf_hex).unwrap());
    let default_leaf_vec = default_leaf_scalar[0].into_repr().to_bytes_be();
    let smt = Smt::new(&pairs, &poseidon, &default_leaf_vec).unwrap();
    let root = smt.root().into_repr().to_bytes_be();
    let hex_root = hex::encode(root);
    let expected_root =
        "304341db4305ca71db912b3ea85acb4ab8f687435aa51a9a65220bfc558eb8d1";
    assert_eq!(hex_root, expected_root);
}
