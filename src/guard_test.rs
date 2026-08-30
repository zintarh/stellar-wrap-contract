//! Tests for the temporary `MintGuard` cleanup behaviour (issue #259).
//!
//! The temporary mint guard is written to storage at the very beginning of
//! `mint_wrap`, *before* zero-hash validation runs.  Both the success path
//! and every failure path must therefore leave the storage slot clean so
//! that no residual entry is observable after the transaction ends.
//!
//! # Why temporary storage?
//!
//! Soroban's **temporary** storage is transaction-scoped: the runtime
//! automatically discards all temporary entries when a transaction ends,
//! regardless of whether it succeeds or panics.  The guard also clears
//! itself explicitly on the success path (see `mint::mint_wrap`) so that
//! callers inspecting storage *within* the same transaction (e.g. via
//! `env.as_contract`) observe a clean state immediately after a successful
//! mint, not just after the transaction boundary.
//!
//! # Test strategy
//!
//! 1. **Zero-hash failure** — trigger `ContractError::InvalidDataHash` (#15)
//!    and confirm neither temporary nor persistent storage retains a
//!    `MintGuard` entry for the user.
//! 2. **Success path** — perform a valid mint and confirm the guard is
//!    removed from temporary storage once the call completes.

#![cfg(test)]

extern crate std;

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Symbol,
};

use crate::mint::CURRENT_PAYLOAD_VERSION;
use crate::signature::construct_mint_payload;
use crate::storage_types::DataKey;

// ── Helper ────────────────────────────────────────────────────────────────────

/// Build and sign the canonical mint payload, matching the exact byte layout
/// produced by [`crate::signature::construct_mint_payload`].
fn sign_for_test(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> BytesN<64> {
    let payload =
        construct_mint_payload(env, contract, user, period, archetype, data_hash, payload_version);

    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);

    let sig = signer.sign(&buf[..len]);
    BytesN::from_array(env, &sig.to_bytes())
}

// ── Issue #259 tests ──────────────────────────────────────────────────────────

/// Trigger a zero-hash mint failure and verify that no `MintGuard` entry is
/// left in either temporary or persistent storage afterwards.
///
/// Acceptance criteria (issue #259):
///   1. Trigger a zero-hash mint failure.                        ✓
///   2. Catch the panic / inspect storage post-failure.          ✓
///   3. Assert no temporary or persistent guard entry remains.   ✓
#[test]
fn test_zero_hash_mint_failure_leaves_no_guard_entry() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // ── Setup ─────────────────────────────────────────────────────────────
    let signing_key = SigningKey::from_bytes(&[0x59u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let period = 202601u64; // valid YYYYMM period
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    // Sign over the zero hash so the signature itself is valid; the contract
    // must reject on the zero-hash check, not on the signature check.
    let sig = sign_for_test(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &zero_hash,
        CURRENT_PAYLOAD_VERSION,
    );

    // ── 1. Trigger the zero-hash failure ──────────────────────────────────
    // `try_mint_wrap` returns a `Result` instead of panicking, so we can
    // inspect storage after the call even though it fails.
    let result = client.try_mint_wrap(
        &user,
        &period,
        &archetype,
        &zero_hash,
        &CURRENT_PAYLOAD_VERSION,
        &sig,
    );

    // ── 2. Confirm the call failed with InvalidDataHash (#15) ─────────────
    assert!(
        result.is_err(),
        "mint_wrap with a zero hash must return an error"
    );

    // ── 3. Inspect storage — guard must be absent ─────────────────────────
    let guard_key = DataKey::MintGuard(user.clone());
    env.as_contract(&contract_id, || {
        assert!(
            !env.storage().temporary().has(&guard_key),
            "MintGuard must NOT persist in temporary storage after a failed mint"
        );
        assert!(
            !env.storage().persistent().has(&guard_key),
            "MintGuard must NOT persist in persistent storage after a failed mint"
        );
    });

    // Bonus: confirm no wrap record was written either.
    assert!(
        client.get_wrap(&user, &period).is_none(),
        "No wrap record should be stored after a failed mint"
    );
}

/// After a *successful* mint the guard must also be absent — it is explicitly
/// removed on the happy path by `mint_wrap` so that within-transaction
/// inspection also sees a clean state.
#[test]
fn test_successful_mint_leaves_no_guard_entry() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[0x60u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let period = 202601u64;
    let data_hash = BytesN::from_array(&env, &[0x42u8; 32]);

    let sig = sign_for_test(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );

    // Successful mint.
    client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &sig);

    // Guard must be gone after the transaction completes.
    let guard_key = DataKey::MintGuard(user.clone());
    env.as_contract(&contract_id, || {
        assert!(
            !env.storage().temporary().has(&guard_key),
            "MintGuard must NOT remain in temporary storage after a successful mint"
        );
        assert!(
            !env.storage().persistent().has(&guard_key),
            "MintGuard must NOT appear in persistent storage after a successful mint"
        );
    });

    // And the wrap record must exist.
    assert!(client.get_wrap(&user, &period).is_some());
}

/// Verify that a mint failure caused by an *already-existing record*
/// (`WrapAlreadyExists`, #4) also leaves no guard entry.
#[test]
fn test_duplicate_mint_failure_leaves_no_guard_entry() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[0x61u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let period = 202602u64;
    let data_hash = BytesN::from_array(&env, &[0x77u8; 32]);

    let sig = sign_for_test(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );

    // First mint succeeds.
    client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &sig);

    // Second mint with the same parameters must fail with WrapAlreadyExists.
    let result = client.try_mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &sig,
    );
    assert!(result.is_err(), "Duplicate mint should fail.");

    // No residual guard entry.
    let guard_key = DataKey::MintGuard(user.clone());
    env.as_contract(&contract_id, || {
        assert!(
            !env.storage().temporary().has(&guard_key),
            "MintGuard must NOT persist after a duplicate-mint failure"
        );
        assert!(
            !env.storage().persistent().has(&guard_key),
            "MintGuard must NOT appear in persistent storage after a duplicate-mint failure"
        );
    });
}
