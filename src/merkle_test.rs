#![cfg(test)]

use soroban_sdk::{testutils::Address as _, vec, xdr::ToXdr, Address, Bytes, BytesN, Env};

use crate::{
    merkle::{compute_whitelist_leaf, hash_pair, WHITELIST_DOMAIN_SEPARATOR},
    ContractError, StellarWrapContract, StellarWrapContractClient,
};

fn setup() -> (Env, StellarWrapContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1; 32]);
    client.initialize(&admin, &admin_pubkey);
    (env, client)
}

#[test]
fn verifies_whitelist_membership_and_single_leaf_proofs() {
    let (env, client) = setup();
    let user = Address::generate(&env);
    let other_user = Address::generate(&env);
    let leaf = client.whitelist_leaf(&user);
    let other_leaf = client.whitelist_leaf(&other_user);
    let root = hash_pair(&env, &leaf, &other_leaf);
    client.set_whitelist_root(&root);

    assert!(client.verify_whitelist(&user, &vec![&env, other_leaf]));
    assert!(!client.verify_whitelist(&user, &vec![&env]));

    client.set_whitelist_root(&leaf);
    assert!(client.verify_whitelist(&user, &vec![&env]));
    assert!(!client.verify_whitelist(&other_user, &vec![&env]));
}

#[test]
fn whitelist_leaf_and_pair_hash_use_domain_prefixes() {
    let env = Env::default();
    let user = Address::generate(&env);
    let mut leaf_input = Bytes::new(&env);
    leaf_input.append(&Bytes::from_array(&env, &[0x00]));
    leaf_input.append(&Bytes::from_array(&env, WHITELIST_DOMAIN_SEPARATOR));
    leaf_input.append(&user.to_xdr(&env));
    let expected_leaf = env.crypto().sha256(&leaf_input);
    assert_eq!(
        compute_whitelist_leaf(&env, &user).to_array(),
        expected_leaf.to_array()
    );

    let a = BytesN::from_array(&env, &[1; 32]);
    let b = BytesN::from_array(&env, &[2; 32]);
    let mut node_input = Bytes::new(&env);
    node_input.append(&Bytes::from_array(&env, &[0x01]));
    node_input.append(&Bytes::from_array(&env, &a.to_array()));
    node_input.append(&Bytes::from_array(&env, &b.to_array()));
    let expected_node = env.crypto().sha256(&node_input);
    assert_eq!(hash_pair(&env, &a, &b).to_array(), expected_node.to_array());
}

#[test]
fn rejects_proofs_over_the_depth_limit() {
    let (env, client) = setup();
    let user = Address::generate(&env);
    let leaf = client.whitelist_leaf(&user);
    client.set_whitelist_root(&leaf);

    let mut proof = soroban_sdk::Vec::new(&env);
    for _ in 0..33 {
        proof.push_back(BytesN::from_array(&env, &[0; 32]));
    }

    assert_eq!(
        client.try_verify_whitelist(&user, &proof),
        Err(Ok(ContractError::MerkleProofTooLong.into()))
    );
}
