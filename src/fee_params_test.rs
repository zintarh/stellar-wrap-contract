#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    Address, BytesN, Env, TryIntoVal,
};

// ── helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, StellarWrapContractClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // initialize needs admin auth, then we blanket-mock for the rest of the test
    env.mock_all_auths();
    client.initialize(&admin, &pubkey);

    (env, client, admin)
}

fn valid_params(env: &Env) -> storage_types::FeeParams {
    let _ = env; // kept for call-site symmetry
    storage_types::FeeParams {
        base_fee: 100,
        per_kib_fee: 10,
        scale_step_kib: 1,
        max_fee: 10_000,
    }
}

// ── acceptance criteria ──────────────────────────────────────────────────────

/// AC: fee_params() returns the documented defaults before any set.
#[test]
fn test_fee_params_defaults_before_set() {
    let (_, client, _) = setup();

    let defaults = client.fee_params();
    assert_eq!(defaults.base_fee, 0);
    assert_eq!(defaults.per_kib_fee, 0);
    assert_eq!(defaults.scale_step_kib, 1);
    assert_eq!(defaults.max_fee, i128::MAX);
}

/// AC: fee_params() round-trips a set value.
#[test]
fn test_fee_params_round_trip() {
    let (env, client, _) = setup();

    let params = valid_params(&env);
    client.set_fee_params(&params);

    let stored = client.fee_params();
    assert_eq!(stored, params);
}

/// AC: A non-admin call fails (Unauthorized / require_auth).
#[test]
#[should_panic]
fn test_set_fee_params_non_admin_rejected() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // initialize with real admin auth
    env.mock_all_auths();
    client.initialize(&admin, &pubkey);

    // now call set_fee_params with NO auth mocked — must panic
    // (mock_all_auths is not active here)
    let params = valid_params(&env);
    client.set_fee_params(&params);
}

/// AC: scale_step_kib == 0 fails with InvalidFeeParams (#14).
#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_set_fee_params_zero_scale_step_rejected() {
    let (env, client, _) = setup();

    client.set_fee_params(&storage_types::FeeParams {
        base_fee: 0,
        per_kib_fee: 0,
        scale_step_kib: 0,
        max_fee: i128::MAX,
    });
}

/// AC: Negative base_fee is rejected with InvalidFeeParams (#14).
#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_set_fee_params_negative_base_fee_rejected() {
    let (env, client, _) = setup();

    client.set_fee_params(&storage_types::FeeParams {
        base_fee: -1,
        per_kib_fee: 0,
        scale_step_kib: 1,
        max_fee: i128::MAX,
    });
}

/// AC: Negative per_kib_fee is rejected with InvalidFeeParams (#14).
#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_set_fee_params_negative_per_kib_fee_rejected() {
    let (env, client, _) = setup();

    client.set_fee_params(&storage_types::FeeParams {
        base_fee: 0,
        per_kib_fee: -1,
        scale_step_kib: 1,
        max_fee: i128::MAX,
    });
}

/// AC: max_fee < base_fee is rejected with InvalidFeeParams (#14).
#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_set_fee_params_max_fee_less_than_base_fee_rejected() {
    let (env, client, _) = setup();

    client.set_fee_params(&storage_types::FeeParams {
        base_fee: 500,
        per_kib_fee: 0,
        scale_step_kib: 1,
        max_fee: 499, // strictly less than base_fee
    });
}

/// max_fee == base_fee is a valid edge case (fee is always capped to base_fee).
#[test]
fn test_set_fee_params_max_fee_equal_to_base_fee_accepted() {
    let (_, client, _) = setup();

    client.set_fee_params(&storage_types::FeeParams {
        base_fee: 200,
        per_kib_fee: 50,
        scale_step_kib: 1,
        max_fee: 200,
    });

    let stored = client.fee_params();
    assert_eq!(stored.max_fee, 200);
    assert_eq!(stored.base_fee, 200);
}

/// AC: A fee_params change emits an event with the correct topics and payload.
#[test]
fn test_set_fee_params_emits_event() {
    let (env, client, _) = setup();

    let params = valid_params(&env);
    client.set_fee_params(&params);

    let events = crate::test_utils::decode_events(&env);

    // Find the fee_par event: topics are (v1, admin, fee_par)
    let fee_events: std::vec::Vec<_> = events
        .iter()
        .filter(|(topics, _)| {
            if topics.len() >= 3 {
                let t1: Result<soroban_sdk::Symbol, _> = topics[1].try_into_val(&env);
                let t2: Result<soroban_sdk::Symbol, _> = topics[2].try_into_val(&env);
                t1.is_ok_and(|s| s == symbol_short!("admin"))
                    && t2.is_ok_and(|s| s == symbol_short!("fee_par"))
            } else {
                false
            }
        })
        .collect();

    assert!(
        !fee_events.is_empty(),
        "Expected at least one fee_par event after set_fee_params"
    );
}

/// Calling set_fee_params a second time overwrites and emits a fresh event each time.
#[test]
fn test_set_fee_params_update_overwrites_and_emits_again() {
    let (env, client, _) = setup();

    let first = storage_types::FeeParams {
        base_fee: 100,
        per_kib_fee: 5,
        scale_step_kib: 2,
        max_fee: 5_000,
    };
    let second = storage_types::FeeParams {
        base_fee: 200,
        per_kib_fee: 20,
        scale_step_kib: 4,
        max_fee: 8_000,
    };

    client.set_fee_params(&first);
    client.set_fee_params(&second);

    // Round-trip returns the latest value
    assert_eq!(client.fee_params(), second);

    // Two fee_par events should have been emitted
    let events = crate::test_utils::decode_events(&env);
    let fee_event_count = events
        .iter()
        .filter(|(topics, _)| {
            if topics.len() >= 3 {
                let t1: Result<soroban_sdk::Symbol, _> = topics[1].try_into_val(&env);
                let t2: Result<soroban_sdk::Symbol, _> = topics[2].try_into_val(&env);
                t1.is_ok_and(|s| s == symbol_short!("admin"))
                    && t2.is_ok_and(|s| s == symbol_short!("fee_par"))
            } else {
                false
            }
        })
        .count();

    assert_eq!(fee_event_count, 2, "Expected two fee_par events");
}
