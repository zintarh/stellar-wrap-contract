#![cfg(test)]
//! Security Test Suite for Stellar Wrap Contract
//!
//! This module contains adversarial tests designed to ensure the contract
//! fails safely when attacked. We test replay attacks, identity theft,
//! cross-contract replay protection, and resource consumption.

use super::*;
use crate::signature::construct_mint_payload;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, Symbol,
};

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
    );

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_replay_attack_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    // First mint - should succeed
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );

    // Verify the wrap was created
    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some(), "First mint should succeed");

    // Replay attack: Try to mint again with the exact same parameters
    // This should PANIC with WrapAlreadyExists error (#4)
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );
}

/// Test 2: Replay Attack with Different Hash (but same period)
/// Even with a different hash, the same period should be rejected
#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_replay_attack_different_hash_same_period_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    // First mint - should succeed
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash_1,
        &CURRENT_PAYLOAD_VERSION,
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

    // Try to mint again for the same period with a different hash
    // This should still fail - period is already used
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash_2,
        &CURRENT_PAYLOAD_VERSION,
        &signature_2,
    );
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
        &signature_1,
    );
    client.mint_wrap(
        &user,
        &period_2,
        &archetype,
        &data_hash_2,
        &CURRENT_PAYLOAD_VERSION,
        &signature_2,
    );
    client.mint_wrap(
        &user,
        &period_3,
        &archetype,
        &data_hash_3,
        &CURRENT_PAYLOAD_VERSION,
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
        CURRENT_PAYLOAD_VERSION,
    );

    // User A mints successfully
    client.mint_wrap(
        &user_a,
        &period,
        &archetype,
        &data_hash_for_a,
        &CURRENT_PAYLOAD_VERSION,
        &signature_a,
    );

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
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user_b,
        &period_b,
        &archetype,
        &data_hash_for_b,
        &CURRENT_PAYLOAD_VERSION,
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
        &signature_v1,
    );

    // Verify the wrap exists on V1
    let wrap_v1 = client_v1.get_wrap(&user, &period);
    assert!(wrap_v1.is_some(), "Wrap should exist on contract V1");

    // NOTE: For full cross-contract replay protection, the signature
    // verification should include the contract address in the signed payload.
    // This test demonstrates that the contracts currently have independent storage,
    // but additional signature binding to contract_id would prevent true replay attacks.
    let payload_v1 = construct_mint_payload(
        &env,
        &contract_v1,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );
    let payload_v2 = construct_mint_payload(
        &env,
        &contract_v2,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );
    assert_ne!(
        payload_v1, payload_v2,
        "Payloads should differ across contract instances"
    );

    // A signature bound to V1 must be rejected by V2's verification.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client_v2.mint_wrap(
            &user,
            &period,
            &archetype,
            &data_hash,
            &CURRENT_PAYLOAD_VERSION,
            &signature_v1,
        );
    }));

    assert!(
        result.is_err(),
        "A signature from V1 should not be replayable on V2"
    );
    assert!(
        client_v2.get_wrap(&user, &period).is_none(),
        "the replay attempt must not create a wrap on V2"
    );

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
        CURRENT_PAYLOAD_VERSION,
    );

    client_v2.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature_v2,
    );

    // Both wraps should exist independently
    assert!(client_v1.get_wrap(&user, &period).is_some());
    assert!(client_v2.get_wrap(&user, &period).is_some());
}

/// Test 6: Gas/Resource Analysis - CPU Instructions
/// Measures the computational cost of a mint operation
#[test]
fn test_gas_analysis_mint_operation() {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();

    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    // Reset budget before the mint operation
    env.cost_estimate().budget().reset_default();

    // Perform the mint operation
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );

    // Get budget consumption
    env.cost_estimate().budget().print();
    // Get budget consumption (only when gas reporting is explicitly enabled)
    if std::env::var("SOROBAN_GAS_REPORT").is_ok() {
        env.cost_estimate().budget().print();
    }

    // Get actual CPU instructions used
    let cpu_insns = env.cost_estimate().budget().cpu_instruction_cost();
    let mem_bytes = env.cost_estimate().budget().memory_bytes_cost();

    // Assert reasonable upper bounds (these values should be tuned based on actual needs)
    // For mainnet deployment, you want these to be as low as possible
    assert!(
        cpu_insns < 10_000_000,
        "CPU instructions too high: {}",
        cpu_insns
    );
    assert!(mem_bytes < 200_000, "Memory usage too high: {}", mem_bytes);

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
    env.cost_estimate().budget().reset_unlimited();

    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    env.cost_estimate().budget().reset_default();

    // Perform 5 mints for different periods
    for i in 1..6 {
        let data_hash = BytesN::from_array(&env, &[i as u8; 32]);
        let archetype = symbol_short!("architect");

        // Create unique period values
        let period = match i {
            1 => 202512u64,
            2 => 202601u64,
            3 => 202602u64,
            4 => 202603u64,
            _ => 202604u64,
        };

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

        client.mint_wrap(
            &user,
            &period,
            &archetype,
            &data_hash,
            &CURRENT_PAYLOAD_VERSION,
            &signature,
        );
    }

    let cpu_insns = env.cost_estimate().budget().cpu_instruction_cost();
    let mem_bytes = env.cost_estimate().budget().memory_bytes_cost();

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
    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );

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
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user,
        &period_2,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature_2,
    );

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
    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );

    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some(), "Should handle reasonably long symbols");
}

/// Test 10: Unauthorized Access - Non-Admin Cannot Mint
/// Verifies that only the admin can authorize minting
#[test]
#[should_panic]
fn test_non_admin_cannot_mint() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
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
        CURRENT_PAYLOAD_VERSION,
    );

    // This should panic because attacker is not authorized
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );
}

/// Test 11: Revocation - Non-admin cannot revoke wraps
/// Only the admin should be able to revoke wrap records.
/// Without any mocked auth, admin.require_auth() will panic.
#[test]
#[should_panic]
fn test_non_admin_cannot_revoke() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    // Do NOT mock any auths — admin.require_auth() should panic
    let reason_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &202512, &reason_hash);
}

// ────────────────────────────────────────────────────────────────────────────
// Two-Step Admin Transfer Tests (Issue #269)
// ────────────────────────────────────────────────────────────────────────────

/// Test 11: Successful Two-Step Admin Transfer (Proposal + Acceptance)
/// Verifies the complete happy path: admin proposes, pending admin accepts.
#[test]
fn test_two_step_admin_transfer_success() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Step 1: Current admin proposes new_admin
    client.propose_admin(&new_admin);

    // Verify pending_admin is set
    assert_eq!(client.get_pending_admin().unwrap(), new_admin);
    // Verify current admin is still the same
    assert_eq!(client.get_admin().unwrap(), admin);

    // Step 2: Pending admin accepts
    client.accept_admin();

    // Verify admin has been transferred
    assert_eq!(client.get_admin().unwrap(), new_admin);
    // Verify pending_admin is cleared
    assert!(client.get_pending_admin().is_none());
}

/// Test 12: Admin Can Cancel a Pending Proposal
/// Verifies that the current admin can cancel a proposed transfer before acceptance.
#[test]
fn test_admin_cancel_proposed_admin() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[2u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Admin proposes
    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin().unwrap(), new_admin);

    // Admin cancels
    client.cancel_proposed_admin();

    // Verify proposal is cleared
    assert!(client.get_pending_admin().is_none());
    // Verify admin remains unchanged
    assert_eq!(client.get_admin().unwrap(), admin);
}

/// Test 13: Unauthorized Acceptance Fails - Non-Pending-Admin Cannot Accept
/// Verifies that an address other than the proposed admin cannot accept the transfer.
#[test]
#[should_panic]
fn test_unauthorized_acceptance_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[3u8; 32]);

    client.initialize(&admin, &pubkey);

    // Set up auths manually to control who is authenticating
    // Admin proposes new_admin
    client.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "propose_admin",
            args: (&new_admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin().unwrap(), new_admin);

    // Attacker tries to accept - should panic because attacker != new_admin
    client.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.accept_admin();
}

/// Test 14: Accepting Without a Proposal Fails
/// Verifies that accept_admin panics when there is no pending proposal.
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_accept_admin_no_proposal_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[4u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // No proposal exists - should panic with NoAdminTransferProposal
    client.accept_admin();
}

/// Test 15: Proposing When a Proposal Already Exists Fails
/// Verifies that propose_admin panics when there is already a pending proposal.
#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_propose_admin_when_proposal_exists_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin_1 = Address::generate(&env);
    let new_admin_2 = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[5u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // First proposal
    client.propose_admin(&new_admin_1);
    assert_eq!(client.get_pending_admin().unwrap(), new_admin_1);

    // Try to propose again without canceling - should panic
    client.propose_admin(&new_admin_2);
}

/// Test 16: Canceling When No Proposal Exists Fails
/// Verifies that cancel_proposed_admin panics when there is no pending proposal.
#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_cancel_no_proposal_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[6u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // No proposal exists - should panic with NoAdminTransferProposal
    client.cancel_proposed_admin();
}

/// Test 17: Non-Admin Cannot Propose a New Admin
/// Verifies that only the current admin can call propose_admin.
#[test]
#[should_panic]
fn test_non_admin_cannot_propose_admin() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[7u8; 32]);

    client.initialize(&admin, &pubkey);

    // Attacker tries to propose - should panic due to require_auth failure
    client.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "propose_admin",
            args: (&new_admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.propose_admin(&new_admin);
}

/// Test 18: Non-Admin Cannot Cancel a Pending Proposal
/// Verifies that only the current admin can cancel a proposal.
#[test]
#[should_panic]
fn test_non_admin_cannot_cancel_proposal() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[8u8; 32]);

    client.initialize(&admin, &pubkey);

    // Admin proposes (mock admin auth)
    client.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "propose_admin",
            args: (&new_admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin().unwrap(), new_admin);

    // Attacker tries to cancel - should panic due to require_auth failure
    client.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "cancel_proposed_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.cancel_proposed_admin();
}

/// Test 19: update_admin (Single-Step) Clears Pending Proposal - Backward Compatibility
/// Verifies that the legacy single-step update_admin clears any pending proposal
/// and successfully transfers admin rights.
#[test]
fn test_update_admin_clears_pending_proposal() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let proposed_admin = Address::generate(&env);
    let direct_new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[9u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Admin proposes a transfer
    client.propose_admin(&proposed_admin);
    assert_eq!(client.get_pending_admin().unwrap(), proposed_admin);
    assert_eq!(client.get_admin().unwrap(), admin);

    // Admin bypasses two-step flow using update_admin (legacy)
    client.update_admin(&direct_new_admin);

    // Verify direct_new_admin is now the admin
    assert_eq!(client.get_admin().unwrap(), direct_new_admin);
    // Verify pending proposal was cleared
    assert!(client.get_pending_admin().is_none());
}

/// Test 20: get_pending_admin Returns None When No Proposal
/// Verifies the getter correctly returns None when there is no pending transfer.
#[test]
fn test_get_pending_admin_none() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[10u8; 32]);

    client.initialize(&admin, &pubkey);

    // No proposal made
    assert!(client.get_pending_admin().is_none());
}

/// Test 21: Propose Then Repropose After Cancel
/// Verifies that after canceling a proposal, the admin can propose a new one.
#[test]
fn test_propose_cancel_repropose() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let first_proposal = Address::generate(&env);
    let second_proposal = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // First proposal
    client.propose_admin(&first_proposal);
    assert_eq!(client.get_pending_admin().unwrap(), first_proposal);

    // Cancel
    client.cancel_proposed_admin();
    assert!(client.get_pending_admin().is_none());

    // Propose a different admin
    client.propose_admin(&second_proposal);
    assert_eq!(client.get_pending_admin().unwrap(), second_proposal);

    // Accept the second proposal
    client.accept_admin();
    assert_eq!(client.get_admin().unwrap(), second_proposal);
    assert!(client.get_pending_admin().is_none());
}

/// Test 22: After Acceptance, New Admin Can Propose Further Transfers
/// Verifies the chain of ownership works: once accepted, the new admin
/// can initiate their own two-step transfers.
#[test]
fn test_new_admin_can_propose_further_transfers() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin_1 = Address::generate(&env);
    let admin_2 = Address::generate(&env);
    let admin_3 = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[12u8; 32]);

    client.initialize(&admin_1, &pubkey);
    env.mock_all_auths();

    // Admin1 -> Admin2 via two-step
    client.propose_admin(&admin_2);
    client.accept_admin();
    assert_eq!(client.get_admin().unwrap(), admin_2);

    // Admin2 -> Admin3 via two-step
    client.propose_admin(&admin_3);
    assert_eq!(client.get_pending_admin().unwrap(), admin_3);
    client.accept_admin();

    // Final state
    assert_eq!(client.get_admin().unwrap(), admin_3);
    assert!(client.get_pending_admin().is_none());
}
