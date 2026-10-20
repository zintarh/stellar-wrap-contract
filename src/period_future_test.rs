//! Tests for the `PeriodInFuture` guard added in issue #1034.
//!
//! `validate_period` now rejects any YYYYMM period whose first second has not
//! yet been reached on the ledger clock, providing defence-in-depth even when
//! the admin signer makes an error.
//!
//! # Acceptance criteria
//! - Future periods are rejected with `ContractError::PeriodInFuture` (code 63).
//! - The current period and all past valid periods are accepted.
//! - The December → January boundary is handled correctly.
//! - `timestamp_to_period` produces the right YYYYMM at well-known timestamps.

#![cfg(test)]

extern crate std;

use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    Address, BytesN, Env,
};

use super::*;
use crate::{
    mint::{timestamp_to_period, validate_period},
    test_utils::sign_payload,
};

// ── timestamp_to_period unit tests ──────────────────────────────────────────

/// The Unix epoch (1970-01-01 00:00:00 UTC) must map to January 1970.
#[test]
fn test_timestamp_to_period_epoch() {
    assert_eq!(timestamp_to_period(0), 197001);
}

/// 2024-01-01 00:00:00 UTC (timestamp 1_704_067_200) → 202401.
#[test]
fn test_timestamp_to_period_jan_2024_start() {
    assert_eq!(timestamp_to_period(1_704_067_200), 202401);
}

/// Last second of January 2024: 2024-01-31 23:59:59 UTC → still 202401.
#[test]
fn test_timestamp_to_period_jan_2024_end() {
    assert_eq!(timestamp_to_period(1_706_745_599), 202401);
}

/// First second of February 2024: 2024-02-01 00:00:00 UTC → 202402.
#[test]
fn test_timestamp_to_period_feb_2024_start() {
    assert_eq!(timestamp_to_period(1_706_745_600), 202402);
}

/// December → January boundary: last second of December 2025 → 202512.
#[test]
fn test_timestamp_to_period_dec_2025_end() {
    // 2025-12-31 23:59:59 UTC = 1_767_225_599
    assert_eq!(timestamp_to_period(1_767_225_599), 202512);
}

/// First second of January 2026: 2026-01-01 00:00:00 UTC → 202601.
#[test]
fn test_timestamp_to_period_jan_2026_start() {
    assert_eq!(timestamp_to_period(1_767_225_600), 202601);
}

/// First second of December 2099: 2099-12-01 00:00:00 UTC → 209912.
#[test]
fn test_timestamp_to_period_dec_2099_start() {
    assert_eq!(timestamp_to_period(4_099_766_400), 209912);
}

// ── validate_period future-rejection tests ───────────────────────────────────

/// Helper: register a minimal contract and set the ledger timestamp.
fn make_env(timestamp: u64) -> Env {
    let env = Env::default();
    env.ledger().set_timestamp(timestamp);
    env
}

/// A period in the current month is accepted (boundary: exactly the current month).
#[test]
fn test_validate_period_current_month_accepted() {
    // Set ledger to mid-January 2025 (2025-01-15 12:00:00 UTC ≈ 1_736_942_400)
    let env = make_env(1_736_942_400);
    let contract_id = env.register(StellarWrapContract, ());
    // period = 202501 (January 2025) — same month as ledger → accepted
    env.as_contract(&contract_id, || {
        validate_period(&env, 202501);
    });
}

/// A past period is accepted.
#[test]
fn test_validate_period_past_month_accepted() {
    // Ledger is in March 2025; period is January 2024 — clearly in the past.
    let env = make_env(1_740_960_000); // 2025-03-03
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 202401);
    });
}

/// A period one month into the future is rejected with PeriodInFuture.
#[test]
#[should_panic(expected = "Error(Contract, #63)")]
fn test_validate_period_one_month_ahead_rejected() {
    // Ledger is at the last second of November 2025.
    // period 202512 (December 2025) has not started yet.
    let env = make_env(1_764_547_199); // 2025-11-30 23:59:59 UTC
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 202512);
    });
}

/// The very first second of a new month is accepted for that month.
#[test]
fn test_validate_period_first_second_of_month_accepted() {
    // 2025-12-01 00:00:00 UTC = 1_764_547_200 → period 202512
    let env = make_env(1_764_547_200);
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 202512);
    });
}

/// January → next year boundary: December 2099 accepted when ledger is in Dec 2099.
#[test]
fn test_validate_period_dec_2099_accepted_in_dec_2099() {
    // 2099-12-01 00:00:00 UTC = 4_099_766_400
    let env = make_env(4_099_766_400);
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 209912);
    });
}

/// January 2026 is rejected when the ledger is still in December 2025.
#[test]
#[should_panic(expected = "Error(Contract, #63)")]
fn test_validate_period_next_year_rejected() {
    // Ledger: 2025-12-31 23:59:59 UTC (last second of December 2025)
    let env = make_env(1_767_225_599);
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 202601); // January 2026 — not yet
    });
}

/// January 2026 is accepted from the very first second of 2026.
#[test]
fn test_validate_period_new_year_accepted_from_first_second() {
    // Ledger: 2026-01-01 00:00:00 UTC = 1_767_225_600
    let env = make_env(1_767_225_600);
    let contract_id = env.register(StellarWrapContract, ());
    env.as_contract(&contract_id, || {
        validate_period(&env, 202601); // accepted exactly at boundary
    });
}

// ── End-to-end mint_wrap tests ───────────────────────────────────────────────

/// Helper: initialise the contract and return a client + signing key.
fn setup_contract(env: &Env) -> (StellarWrapContractClient, Address, SigningKey) {
    let signing_key = SigningKey::from_bytes(&[42u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    (client, contract_id, signing_key)
}

/// `mint_wrap` succeeds for the current month.
#[test]
fn test_mint_wrap_current_period_succeeds() {
    // Ledger is mid-January 2025.
    let env = make_env(1_736_942_400);
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let period = 202501u64; // current month on the ledger

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

    assert!(client.get_wrap(&user, &period).is_some());
}

/// `mint_wrap` with a period one month ahead is rejected with `PeriodInFuture` (code 63).
#[test]
fn test_mint_wrap_future_period_rejected() {
    // Ledger is at the last second of November 2025.
    let env = make_env(1_764_547_199);
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[2u8; 32]);
    let period = 202512u64; // December 2025 — not started yet

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    let result = client.try_mint_wrap(
        &user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig,
    );

    assert_eq!(
        result.unwrap_err(),
        Ok(ContractError::PeriodInFuture),
        "expected PeriodInFuture (#63) for a future-month period"
    );
    assert!(client.get_wrap(&user, &period).is_none());
}

/// `mint_wrap` succeeds once the clock crosses into the next month.
#[test]
fn test_mint_wrap_accepted_from_first_second_of_month() {
    // Ledger: 2025-12-01 00:00:00 UTC (first second of December 2025)
    let env = make_env(1_764_547_200);
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[3u8; 32]);
    let period = 202512u64;

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

    assert!(client.get_wrap(&user, &period).is_some());
}

/// December → January: last second of December 2025 rejects January 2026.
#[test]
fn test_mint_wrap_rejects_next_year_in_last_second_of_current_year() {
    let env = make_env(1_767_225_599); // 2025-12-31 23:59:59 UTC
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[4u8; 32]);
    let period = 202601u64; // January 2026

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    let result = client.try_mint_wrap(
        &user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig,
    );

    assert_eq!(
        result.unwrap_err(),
        Ok(ContractError::PeriodInFuture),
        "January 2026 must be rejected while ledger is still in December 2025"
    );
}

/// December → January: January 2026 is accepted from the very first second of 2026.
#[test]
fn test_mint_wrap_accepts_january_from_first_second_of_new_year() {
    let env = make_env(1_767_225_600); // 2026-01-01 00:00:00 UTC
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[5u8; 32]);
    let period = 202601u64; // January 2026

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

    assert!(client.get_wrap(&user, &period).is_some());
}

/// `mint_wrap` for a far-future period (e.g. 209912) is rejected when the
/// ledger time is decades in the past relative to that period.
#[test]
fn test_mint_wrap_far_future_period_rejected() {
    // Ledger is mid-January 2025; period is December 2099 — 74 years away.
    let env = make_env(1_736_942_400); // mid-Jan 2025
    let (client, contract_id, signing_key) = setup_contract(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[6u8; 32]);
    let period = 209912u64; // December 2099 — far future from Jan 2025

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    let result = client.try_mint_wrap(
        &user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig,
    );

    assert_eq!(
        result.unwrap_err(),
        Ok(ContractError::PeriodInFuture),
        "expected PeriodInFuture for a period 74 years in the future"
    );
}
