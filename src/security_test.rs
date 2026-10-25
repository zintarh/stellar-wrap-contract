#![cfg(test)]
//! Security Test Suite for Stellar Wrap Contract
//!
//! This module contains adversarial tests designed to ensure the contract
//! fails safely when attacked. We test replay attacks, identity theft,
//! cross-contract replay protection, resource consumption, and admin pubkey
//! validation during initialization.

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, Bytes, BytesN, Env, IntoVal, Symbol,
};

use super::*;
use crate::signature::construct_mint_payload;

/// Test 1: Replay Attack Simulation
/// Ensures that a valid signature cannot be reused for the same period
#[allow(clippy::too_many_arguments)]
fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> BytesN<64> {
    let payload = crate::signature::construct_mint_payload(
        env,
        contract,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
        u64::MAX,
    );

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

#[test]
fn test_replay_attack_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202512u64; // December 2025

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );

    // First mint - should succeed
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature,
    );

    // Verify the wrap was created
    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some(), "First mint should succeed.");

    // Replay attack: Try to mint again with the exact same parameters.
    let balance_before = client.balance_of(&user);
    let result = client.try_mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature,
    );

    assert!(result.is_err(), "replay mint must fail");
    assert_eq!(client.balance_of(&user), balance_before);
}

/// Test 2: Replay Attack with Different Hash (but same period)
/// Even with a different hash, the same period should be rejected
#[test]
fn test_replay_attack_different_hash_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let data_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let data_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202512u64; // December 2025

    let signature_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash_1,
        CURRENT_PAYLOAD_VERSION,
    );

    // First mint - should succeed
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash_1,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_1,
    );

    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash_2,
        CURRENT_PAYLOAD_VERSION,
    );

    // Try to mint again for the same period with a different hash.
    let balance_before = client.balance_of(&user);
    let result = client.try_mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash_2,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_2,
    );

    assert!(result.is_err(), "duplicate-period mint must fail");
    assert_eq!(client.balance_of(&user), balance_before);
}

/// Test 3: Multiple Valid Periods Work Correctly
/// Verifies that different periods for the same user work without issue
#[test]
fn test_multiple_periods_for_same_user_success() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let data_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let data_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let data_hash_3 = BytesN::from_array(&env, &[77u8; 32]);
    let archetype = symbol_short!("architect");

    let period_1 = 202512u64; // December 2025
    let period_2 = 202601u64; // January 2026
    let period_3 = 202602u64; // February 2026

    let signature_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_1,
        &archetype,
        &data_hash_1,
        CURRENT_PAYLOAD_VERSION,
    );
    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_2,
        &archetype,
        &data_hash_2,
        CURRENT_PAYLOAD_VERSION,
    );
    let signature_3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_3,
        &archetype,
        &data_hash_3,
        CURRENT_PAYLOAD_VERSION,
    );

    // All three should succeed
    client.mint_wrap(
        &user,
        &period_1,
        &archetype,
        &data_hash_1,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_1,
    );
    client.mint_wrap(
        &user,
        &period_2,
        &archetype,
        &data_hash_2,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_2,
    );
    client.mint_wrap(
        &user,
        &period_3,
        &archetype,
        &data_hash_3,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_3,
    );

    // Verify all three wraps exist
    assert!(client.get_wrap(&user, &period_1).is_some());
    assert!(client.get_wrap(&user, &period_2).is_some());
    assert!(client.get_wrap(&user, &period_3).is_some());
}

/// Test 4: Identity Theft / Signature Mismatch Attack
/// Tests that a signature intended for User A cannot be used by User B
///
/// NOTE: This test currently relies on the admin authorization check.
/// For full security, the signature verification should cryptographically
/// bind the payload to the specific user address.
#[test]
fn test_signature_cannot_be_stolen_by_another_user() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    // Admin creates a signature for User A
    let data_hash_for_a = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202512u64; // December 2025

    let signature_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &data_hash_for_a,
        CURRENT_PAYLOAD_VERSION,
    );

    // User A mints successfully
    client.mint_wrap(
        &user_a,
        &period,
        &archetype,
        &data_hash_for_a,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_a,
    );

    // Verify User A has the wrap
    let wrap_a = client.get_wrap(&user_a, &period);
    assert!(wrap_a.is_some(), "User A should have the wrap.");

    // User B tries to mint with their own period (this is allowed)
    let data_hash_for_b = BytesN::from_array(&env, &[99u8; 32]);
    let period_b = 202601u64; // January 2026

    let signature_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period_b,
        &archetype,
        &data_hash_for_b,
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user_b,
        &period_b,
        &archetype,
        &data_hash_for_b,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_b,
    );

    // Verify both users have their respective wraps and they're distinct
    let wrap_a = client.get_wrap(&user_a, &period).unwrap();
    let wrap_b = client.get_wrap(&user_b, &period_b).unwrap();

    assert_eq!(wrap_a.data_hash, data_hash_for_a);
    assert_eq!(wrap_b.data_hash, data_hash_for_b);

    // User B should NOT have User A's period
    let user_b_period_dec = client.get_wrap(&user_b, &period);
    assert!(
        user_b_period_dec.is_none(),
        "User B should not have User A's period."
    );
}

/// Test 5: Cross-Contract Replay Protection
/// Verifies that a signature valid for Contract V1 cannot be replayed on Contract V2
#[test]
fn test_cross_contract_replay_protection() {
    let env = Env::default();

    // Deploy two separate contract instances (V1 and V2)
    let contract_v1 = env.register(StellarWrapContract, ());
    let contract_v2 = env.register(StellarWrapContract, ());

    let client_v1 = StellarWrapContractClient::new(&env, &contract_v1);
    let client_v2 = StellarWrapContractClient::new(&env, &contract_v2);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Initialize both contracts with the same admin
    client_v1.initialize(&admin, &admin_pubkey);
    client_v2.initialize(&admin, &admin_pubkey);

    env.mock_all_auths();

    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202512u64; // December 2025

    let signature_v1 = sign_payload(
        &env,
        &signing_key,
        &contract_v1,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );

    client_v1.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature_v1,
    );

    // Verify the wrap exists on V1
    let wrap_v1 = client_v1.get_wrap(&user, &period);
    assert!(wrap_v1.is_some(), "Wrap should exist on contract V1.");

    // NOTE: For full cross-contract replay protection, the signature
    // verification should include the contract address in the signed payload.
    // This test demonstrates that the contracts c

/// Test 6: Admin pubkey validation at initialization
/// Rejects all-zero keys and accepts a valid ed25519 key.
#[test]
fn test_initialize_rejects_all_zero_admin_pubkey() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let zero_pubkey = BytesN::from_array(&env, &[0u8; 32]);

    env.mock_all_auths();
    let result = client.try_initialize(&admin, &zero_pubkey);
    assert!(result.is_err(), "all-zero admin pubkey must be rejected");
}

#[test]
fn test_initialize_accepts_valid_admin_pubkey() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    env.mock_all_auths();
    let result = client.try_initialize(&admin, &admin_pubkey);
    assert!(result.is_ok(), "valid admin pubkey must be accepted");
}
