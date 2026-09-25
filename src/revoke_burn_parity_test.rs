//! Acceptance-criterion tests for the `remove_wrap_record` refactor (issue #720).
//!
//! Verifies that `mint_wrap` followed by `revoke_wrap` and `mint_wrap` followed
//! by `burn_wrap` leave **byte-identical** observable contract state for the
//! same `(user, period)` record.
//!
//! "Byte-identical state" is validated by comparing every public read-side
//! observable for the affected user and the global counters:
//! - `get_wrap` → `None`
//! - `balance_of` → 0
//! - `get_latest_wrap` → `None`
//! - `total_wrap_count` → unchanged global count
//! - `storage_bytes` → same value in both paths
//! - `get_last_updated` → `Some(_)` (both paths now set it)

#![cfg(test)]

extern crate std;

use super::*;
use crate::test_utils::sign_payload;
use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::{Address as _, Ledger}, Address, BytesN, Env};

/// Shared fixture: initialise a fresh contract, mint one wrap for `user` on
/// `period`, and return the client alongside the admin and user addresses.
fn setup_minted_wrap(
    env: &Env,
    key_seed: u8,
) -> (
    StellarWrapContractClient<'_>,
    Address, // contract_id (as address)
    Address, // admin
    Address, // user
) {
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[key_seed; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let user = Address::generate(env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let signature = sign_payload(
        env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);

    (client, contract_id, admin, user)
}

const PERIOD: u64 = 202401u64;

/// Capture observable post-removal state as a plain tuple that can be compared.
///
/// Returns `(wrap_exists, balance, latest_wrap_exists, total_wrap_count, storage_bytes, last_updated_set)`.
fn observe(client: &StellarWrapContractClient, user: &Address) -> (bool, i128, bool, u32, u64, bool) {
    let wrap_exists = client.get_wrap(user, &PERIOD).is_some();
    let balance = client.balance_of(user);
    let latest_wrap_exists = client.get_latest_wrap(user).is_some();
    let total = client.total_wrap_count();
    let bytes = client.storage_bytes();
    let last_updated = client.get_last_updated(user).is_some();
    (wrap_exists, balance, latest_wrap_exists, total, bytes, last_updated)
}

#[test]
fn test_revoke_and_burn_leave_identical_state() {
    // ── Path A: mint → revoke ────────────────────────────────────────────
    let env_a = Env::default();
    let (client_a, _, _, user_a) = setup_minted_wrap(&env_a, 0xAA);
    let reason = BytesN::from_array(&env_a, &[0u8; 32]);
    client_a.revoke_wrap(&user_a, &PERIOD, &reason);
    let state_a = observe(&client_a, &user_a);

    // ── Path B: mint → burn ──────────────────────────────────────────────
    let env_b = Env::default();
    let (client_b, _, _, user_b) = setup_minted_wrap(&env_b, 0xBB);
    client_b.burn_wrap(&user_b, &PERIOD);
    let state_b = observe(&client_b, &user_b);

    // ── Assert identical observable state ────────────────────────────────
    let (wrap_a, bal_a, latest_a, total_a, bytes_a, last_upd_a) = state_a;
    let (wrap_b, bal_b, latest_b, total_b, bytes_b, last_upd_b) = state_b;

    assert!(!wrap_a, "revoke: wrap should not exist after removal");
    assert!(!wrap_b, "burn:   wrap should not exist after removal");

    assert_eq!(bal_a, 0, "revoke: balance should be 0");
    assert_eq!(bal_b, 0, "burn:   balance should be 0");
    assert_eq!(
        bal_a, bal_b,
        "revoke and burn must yield the same balance"
    );

    assert!(
        !latest_a,
        "revoke: latest_wrap should be None when only wrap was removed"
    );
    assert!(
        !latest_b,
        "burn:   latest_wrap should be None when only wrap was removed"
    );
    assert_eq!(
        latest_a, latest_b,
        "revoke and burn must yield the same latest_wrap presence"
    );

    assert_eq!(total_a, 0, "revoke: TotalWrapCount should be 0");
    assert_eq!(total_b, 0, "burn:   TotalWrapCount should be 0");
    assert_eq!(
        total_a, total_b,
        "revoke and burn must yield the same TotalWrapCount"
    );

    assert_eq!(
        bytes_a, bytes_b,
        "revoke and burn must yield identical storage_bytes accounting"
    );

    assert!(
        last_upd_a,
        "revoke: LastUpdated should be set after removal"
    );
    assert!(last_upd_b, "burn:   LastUpdated should be set after removal");
    assert_eq!(
        last_upd_a, last_upd_b,
        "revoke and burn must both set LastUpdated"
    );
}

/// Verify that the global TotalWrapCount decrements correctly when a wrap is
/// removed via either revoke or burn (previously neither path decremented it).
#[test]
fn test_revoke_decrements_total_wrap_count() {
    let env = Env::default();
    let (client, _, _, user) = setup_minted_wrap(&env, 0x01);

    assert_eq!(client.total_wrap_count(), 1, "should have 1 wrap after mint");
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &PERIOD, &reason);
    assert_eq!(
        client.total_wrap_count(),
        0,
        "revoke should decrement TotalWrapCount"
    );
}

#[test]
fn test_burn_decrements_total_wrap_count() {
    let env = Env::default();
    let (client, _, _, user) = setup_minted_wrap(&env, 0x02);

    assert_eq!(client.total_wrap_count(), 1, "should have 1 wrap after mint");
    client.burn_wrap(&user, &PERIOD);
    assert_eq!(
        client.total_wrap_count(),
        0,
        "burn should decrement TotalWrapCount"
    );
}

/// Verify that burn now sets LastUpdated (previously it did not).
#[test]
fn test_burn_sets_last_updated() {
    let env = Env::default();
    let (client, _, _, user) = setup_minted_wrap(&env, 0x03);

    // Advance ledger time so the mint's timestamp and the burn's timestamp differ.
    env.ledger().with_mut(|l| l.timestamp = 10_000);
    client.burn_wrap(&user, &PERIOD);
    assert!(
        client.get_last_updated(&user).is_some(),
        "burn must set LastUpdated"
    );
}
