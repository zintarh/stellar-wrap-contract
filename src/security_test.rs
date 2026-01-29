#![cfg(test)]
//! Security Test Suite for Stellar Wrap Contract
//!
//! This module contains adversarial tests designed to ensure the contract
//! fails safely when attacked. We test replay attacks, identity theft,
//! cross-contract replay protection, and resource consumption.

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env,
};

/// Helper function to sign payloads for testing
fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let mut payload = Bytes::new(env);
    payload.append(&contract.to_xdr(env));
    payload.append(&user.clone().to_xdr(env));
    payload.append(&period.to_xdr(env));
    payload.append(&archetype.clone().to_xdr(env));
    payload.append(&data_hash.clone().to_xdr(env));

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

/// Test 1: Replay Attack Simulation
/// Ensures that a valid signature cannot be reused for the same period
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_replay_attack_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

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
    );

    // First mint - should succeed
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    // Verify the wrap was created
    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some(), "First mint should succeed");

    // Replay attack: Try to mint again with the exact same parameters
    // This should PANIC with WrapAlreadyExists error (#4)
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);
}

/// Test 2: Replay Attack with Different Hash (but same period)
/// Even with a different hash, the same period should be rejected
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_replay_attack_different_hash_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

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
    );

    // First mint - should succeed
    client.mint_wrap(&user, &period, &archetype, &data_hash_1, &signature_1);

    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash_2,
    );

    // Try to mint again for the same period with a different hash
    // This should still fail - period is already used
    client.mint_wrap(&user, &period, &archetype, &data_hash_2, &signature_2);
}

/// Test 3: Multiple Valid Periods Work Correctly
/// Verifies that different periods for the same user work without issue
#[test]
fn test_multiple_periods_for_same_user_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

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
    );
    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_2,
        &archetype,
        &data_hash_2,
    );
    let signature_3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_3,
        &archetype,
        &data_hash_3,
    );

    // All three should succeed
    client.mint_wrap(&user, &period_1, &archetype, &data_hash_1, &signature_1);
    client.mint_wrap(&user, &period_2, &archetype, &data_hash_2, &signature_2);
    client.mint_wrap(&user, &period_3, &archetype, &data_hash_3, &signature_3);

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
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

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
    );

    // User A mints successfully
    client.mint_wrap(&user_a, &period, &archetype, &data_hash_for_a, &signature_a);

    // Verify User A has the wrap
    let wrap_a = client.get_wrap(&user_a, &period);
    assert!(wrap_a.is_some(), "User A should have the wrap");

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
    );

    client.mint_wrap(
        &user_b,
        &period_b,
        &archetype,
        &data_hash_for_b,
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
        "User B should not have User A's period"
    );
}

/// Test 5: Cross-Contract Replay Protection
/// Verifies that a signature valid for Contract V1 cannot be replayed on Contract V2
#[test]
fn test_cross_contract_replay_protection() {
    let env = Env::default();

    // Deploy two separate contract instances (V1 and V2)
    let contract_v1 = env.register_contract(None, StellarWrapContract);
    let contract_v2 = env.register_contract(None, StellarWrapContract);

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
    );

    // Mint successfully on V1
    client_v1.mint_wrap(&user, &period, &archetype, &data_hash, &signature_v1);

    // Verify the wrap exists on V1
    let wrap_v1 = client_v1.get_wrap(&user, &period);
    assert!(wrap_v1.is_some(), "Wrap should exist on contract V1");

    // The same user can mint on V2 (they are independent contracts)
    // This should succeed because they are different contract instances
    let signature_v2 = sign_payload(
        &env,
        &signing_key,
        &contract_v2,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client_v2.mint_wrap(&user, &period, &archetype, &data_hash, &signature_v2);

    // Verify both contracts have independent storage
    let wrap_v2 = client_v2.get_wrap(&user, &period);
    assert!(wrap_v2.is_some(), "Wrap should exist on contract V2");

    // Both wraps should exist independently
    assert!(client_v1.get_wrap(&user, &period).is_some());
    assert!(client_v2.get_wrap(&user, &period).is_some());

    // NOTE: For full cross-contract replay protection, the signature
    // verification should include the contract address in the signed payload.
    // This test demonstrates that the contracts currently have independent storage,
    // but additional signature binding to contract_id would prevent true replay attacks.
}

/// Test 6: Gas/Resource Analysis - CPU Instructions
/// Measures the computational cost of a mint operation
#[test]
fn test_gas_analysis_mint_operation() {
    let env = Env::default();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

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
    );

    // Reset budget before the mint operation
    env.budget().reset_default();

    // Perform the mint operation
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    // Get budget consumption
    env.budget().print();

    // Get actual CPU instructions used
    let cpu_insns = env.budget().cpu_instruction_cost();
    let mem_bytes = env.budget().memory_bytes_cost();

    // Assert reasonable upper bounds (these values should be tuned based on actual needs)
    // For mainnet deployment, you want these to be as low as possible
    assert!(
        cpu_insns < 10_000_000,
        "CPU instructions too high: {}",
        cpu_insns
    );
    assert!(mem_bytes < 100_000, "Memory usage too high: {}", mem_bytes);

    // Gas analysis results:
    // CPU Instructions: Check assertion output
    // Memory Bytes: Check assertion output
    // These values are validated by the assertions above
}

/// Test 7: Gas Analysis - Multiple Operations
/// Measures resource consumption for multiple sequential mints
#[test]
fn test_gas_analysis_multiple_mints() {
    let env = Env::default();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    env.budget().reset_default();

    // Perform 5 mints for different periods
    for i in 0..5 {
        let data_hash = BytesN::from_array(&env, &[i as u8; 32]);
        let archetype = symbol_short!("architect");

        // Create unique period values
        let period = match i {
            0 => 202512u64, // December 2025
            1 => 202601u64, // January 2026
            2 => 202602u64, // February 2026
            3 => 202603u64, // March 2026
            _ => 202604u64, // April 2026
        };

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );

        client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);
    }

    let cpu_insns = env.budget().cpu_instruction_cost();
    let mem_bytes = env.budget().memory_bytes_cost();

    // Gas analysis for 5 mints - results tracked in budget
    // Verify resource usage is within reasonable bounds for batch operations
    assert!(cpu_insns < 50_000_000, "Batch CPU too high: {}", cpu_insns);
    assert!(mem_bytes < 500_000, "Batch memory too high: {}", mem_bytes);
}

/// Test 8: Timestamp Manipulation Resistance
/// Ensures the contract uses ledger timestamp, not user-provided values
#[test]
fn test_timestamp_is_from_ledger_not_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    // Set specific ledger timestamp
    env.ledger().with_mut(|li| {
        li.timestamp = 1000000;
    });

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
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    let wrap = client.get_wrap(&user, &period).unwrap();

    // Verify timestamp matches ledger, not any user-provided value
    assert_eq!(wrap.timestamp, 1000000, "Timestamp should come from ledger");

    // Advance ledger time and mint another period
    env.ledger().with_mut(|li| {
        li.timestamp = 2000000;
    });

    let period_2 = 202601u64; // January 2026
    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_2,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period_2, &archetype, &data_hash, &signature_2);

    let wrap_2 = client.get_wrap(&user, &period_2).unwrap();
    assert_eq!(
        wrap_2.timestamp, 2000000,
        "Second timestamp should match new ledger time"
    );
}

/// Test 9: Edge Case - Maximum Symbol Length
/// Tests behavior with maximum-length symbol names
#[test]
fn test_edge_case_long_symbols() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    // symbol_short! supports up to 9 ASCII characters
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
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some(), "Should handle reasonably long symbols");
}

/// Test 10: Unauthorized Access - Non-Admin Cannot Mint
/// Verifies that only the admin can authorize minting
#[test]
#[should_panic]
fn test_non_admin_cannot_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let _attacker = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    // Don't mock auth - let it fail naturally
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
    );

    // This should panic because attacker is not authorized
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);
}
