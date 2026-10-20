#![cfg(test)]

extern crate std;

use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::{Address as _, Ledger}, Address, BytesN, Env};

use super::*;
use crate::test_utils::sign_payload_versioned;

// Use a reference timestamp well inside January 2024 so period 202401 passes
// the future-period guard. All three boundary tests below vary `now` relative
// to `valid_until`; the period itself stays fixed at 202401.
//
// 1_704_100_000 ≈ 2024-01-01 09:06:40 UTC — comfortably inside month 202401.
const BASE_TS: u64 = 1_704_100_000;
const VALID_UNTIL: u64 = BASE_TS + 1_000; // deadline one thousand seconds later
const PERIOD: u64 = 202401u64;

fn setup(env: &Env) -> (StellarWrapContractClient, Address, SigningKey) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let signing_key = SigningKey::from_bytes(&[55u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    (client, contract_id, signing_key)
}

/// Ledger is before `valid_until` → mint succeeds.
#[test]
fn test_mint_succeeds_before_expiry() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = BASE_TS); // now < valid_until
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[1u8; 32]);

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, PERIOD, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, VALID_UNTIL,
    );

    client.mint_wrap(&user, &PERIOD, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &VALID_UNTIL, &sig);
    assert!(client.get_wrap(&user, &PERIOD).is_some());
}

/// Ledger equals `valid_until` exactly → still valid (gate is `now > valid_until`).
#[test]
fn test_mint_succeeds_at_exact_deadline() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = VALID_UNTIL); // now == valid_until
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[2u8; 32]);

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, PERIOD, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, VALID_UNTIL,
    );

    client.mint_wrap(&user, &PERIOD, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &VALID_UNTIL, &sig);
    assert!(client.get_wrap(&user, &PERIOD).is_some());
}

/// Ledger is one second past `valid_until` → rejected with SignatureExpired (#62).
#[test]
fn test_mint_fails_one_second_after_deadline() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = VALID_UNTIL + 1); // now > valid_until
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[3u8; 32]);

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, PERIOD, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, VALID_UNTIL,
    );

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_wrap(&user, &PERIOD, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &VALID_UNTIL, &sig);
    }));

    assert!(result.is_err(), "mint must be rejected after valid_until");
    let err = result.unwrap_err().downcast::<String>().unwrap_or_default();
    assert!(
        err.contains("Error(Contract, #62)"),
        "expected SignatureExpired (#62), got: {err}"
    );
    assert!(client.get_wrap(&user, &PERIOD).is_none());
}
