#![cfg(test)]

extern crate std;

use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::{Address as _, Ledger}, Address, BytesN, Env};

use super::*;
use crate::test_utils::sign_payload_versioned;

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

#[test]
fn test_mint_succeeds_before_expiry() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1000);
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let period = 202501u64;
    let valid_until = 2000u64; // expires at 2000, now is 1000

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, period, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, valid_until,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &valid_until, &sig);
    assert!(client.get_wrap(&user, &period).is_some());
}

#[test]
fn test_mint_succeeds_at_exact_deadline() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 2000);
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[2u8; 32]);
    let period = 202502u64;
    let valid_until = 2000u64; // now == valid_until: still valid (not strictly >)

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, period, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, valid_until,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &valid_until, &sig);
    assert!(client.get_wrap(&user, &period).is_some());
}

#[test]
fn test_mint_fails_one_second_after_deadline() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 2001);
    let (client, contract_id, signing_key) = setup(&env);

    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[3u8; 32]);
    let period = 202503u64;
    let valid_until = 2000u64; // expired 1 second ago

    let sig = sign_payload_versioned(
        &env, &signing_key, &contract_id, &user, period, &archetype, &hash,
        CURRENT_PAYLOAD_VERSION, valid_until,
    );

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period, &archetype, &hash, &CURRENT_PAYLOAD_VERSION, &valid_until, &sig);
    }));

    assert!(result.is_err(), "mint must be rejected after valid_until");
    let err = result.unwrap_err().downcast::<String>().unwrap_or_default();
    assert!(
        err.contains("Error(Contract, #57)"),
        "expected SignatureExpired (#57), got: {err}"
    );
    assert!(client.get_wrap(&user, &period).is_none());
}
