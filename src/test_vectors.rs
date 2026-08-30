#![cfg(test)]

//! Deterministic Ed25519 mint-signature fixtures (Issue #275).
//!
//! Fixed seeds and field values let backend/frontend/contract implementations
//! confirm they encode the same canonical mint payload.
//!
//! ## Regenerating fixtures
//!
//! 1. Keep `FIXTURE_SECRET_SEED`, `FIXTURE_PERIOD`, `FIXTURE_DATA_HASH`, and
//!    `FIXTURE_PAYLOAD_VERSION` as the canonical inputs (archetype: `"builder"`).
//! 2. Register the contract in `Env::default()` and take the first
//!    `Address::generate` user (generation order is deterministic).
//! 3. Build the payload with `construct_mint_payload` and sign with
//!    `SigningKey::from_bytes(&FIXTURE_SECRET_SEED)`.
//! 4. Capture pubkey / payload / signature hex via the ignored printer test:
//!
//! ```bash
//! cargo test --lib gen_print_mint_fixtures -- --ignored --nocapture
//! ```

extern crate std;

use super::*;
use crate::mint::CURRENT_PAYLOAD_VERSION;
use crate::signature::{construct_mint_payload, verify_mint_signature, MINT_DOMAIN_SEPARATOR};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};

/// Fixed Ed25519 secret-key seed (tests only).
pub const FIXTURE_SECRET_SEED: [u8; 32] = [0x42; 32];

/// Period (`YYYYMM`).
pub const FIXTURE_PERIOD: u64 = 202412;

/// Payload version required by `mint_wrap`.
pub const FIXTURE_PAYLOAD_VERSION: u32 = CURRENT_PAYLOAD_VERSION;

/// Non-zero data hash fixture.
pub const FIXTURE_DATA_HASH: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
];

fn hex_encode(bytes: &[u8]) -> std::string::String {
    use std::fmt::Write;
    let mut out = std::string::String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn fixture_archetype() -> Symbol {
    symbol_short!("builder")
}

fn setup_fixture_env() -> (Env, Address, Address, BytesN<32>, BytesN<64>, Bytes) {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let user = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&FIXTURE_SECRET_SEED);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    let payload = construct_mint_payload(
        &env,
        &contract_id,
        &user,
        FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        FIXTURE_PAYLOAD_VERSION,
    );

    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);
    let signature = BytesN::from_array(&env, &signing_key.sign(&buf[..len]).to_bytes());

    (env, contract_id, user, pubkey, signature, payload)
}

#[test]
fn test_deterministic_fixture_used_in_contract_mint() {
    let (env, contract_id, user, pubkey, signature, payload) = setup_fixture_env();

    let mut head = [0u8; 15];
    for (i, slot) in head.iter_mut().enumerate() {
        *slot = payload.get(i as u32).unwrap();
    }
    assert_eq!(&head, MINT_DOMAIN_SEPARATOR);

    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    assert!(verify_mint_signature(
        &env,
        &pubkey,
        &contract_id,
        &user,
        FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        FIXTURE_PAYLOAD_VERSION,
        &signature,
    )
    .is_ok());

    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.mint_wrap(
        &user,
        &FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        &FIXTURE_PAYLOAD_VERSION,
        &signature,
    );
    assert!(client.get_wrap(&user, &FIXTURE_PERIOD).is_some());
}

#[test]
fn test_fixture_inputs_are_stable() {
    assert_eq!(FIXTURE_PERIOD, 202412);
    assert_eq!(FIXTURE_PAYLOAD_VERSION, 1);
    assert_ne!(FIXTURE_DATA_HASH, [0u8; 32]);
    assert_eq!(FIXTURE_SECRET_SEED, [0x42; 32]);
}

#[test]
#[ignore]
fn gen_print_mint_fixtures() {
    let (_env, contract_id, user, pubkey, signature, payload) = setup_fixture_env();
    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);

    std::println!("pubkey_hex={}", hex_encode(&pubkey.to_array()));
    std::println!("payload_hex={}", hex_encode(&buf[..len]));
    std::println!("signature_hex={}", hex_encode(&signature.to_array()));
    std::println!("contract={:?}", contract_id);
    std::println!("user={:?}", user);
}

#[test]
fn test_merkle_leaf_node_domain_separation() {
    let env = Env::default();

    // Leaf payloads are arbitrary; the domain prefix is what separates them
    // from internal node hashes.
    let leaf0 = [0x00u8; 1];
    let leaf1 = [0x01u8; 1];
    let leaf2 = [0x02u8; 1];
    let leaf3 = [0x03u8; 1];

    let h0 = crate::merkle::hash_leaf(&env, &leaf0);
    let h1 = crate::merkle::hash_leaf(&env, &leaf1);
    let h2 = crate::merkle::hash_leaf(&env, &leaf2);
    let h3 = crate::merkle::hash_leaf(&env, &leaf3);

    // Internal node hashes must not collide with leaf hashes.
    assert_ne!(h0, crate::merkle::hash_pair(&env, &h0, &h1));

    let h01 = crate::merkle::hash_pair(&env, &h0, &h1);
    let h23 = crate::merkle::hash_pair(&env, &h2, &h3);
    let root = crate::merkle::hash_pair(&env, &h01, &h23);

    let proof = vec![h1, h23];
    assert!(
        crate::merkle::verify_merkle_proof(&env, &root, &leaf0, &proof).is_ok(),
        "proof for leaf0 should verify against root"
    );
}

#[test]
fn test_merkle_empty_proof_single_leaf() {
    let env = Env::default();
    let leaf = [0x42u8; 1];
    let root = crate::merkle::hash_leaf(&env, &leaf);
    assert!(
        crate::merkle::verify_merkle_proof(&env, &root, &leaf, &Vec::new()).is_ok(),
        "empty proof should be valid for a single-leaf tree"
    );
}

#[test]
fn test_merkle_proof_length_limit() {
    let env = Env::default();
    let leaf = [0x00u8; 1];
    let root = crate::merkle::hash_leaf(&env, &leaf);
    let proof: Vec<_> = (0..=crate::merkle::MAX_PROOF_DEPTH)
        .map(|i| crate::merkle::hash_leaf(&env, &[i as u8]))
        .collect();
    assert!(
        crate::merkle::verify_merkle_proof(&env, &root, &leaf, &proof).is_err(),
        "proofs longer than MAX_PROOF_DEPTH must be rejected"
    );
}
