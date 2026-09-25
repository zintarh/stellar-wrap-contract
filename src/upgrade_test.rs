#![cfg(test)]

//! Tests for the WASM upgrade path and the `contract_version()` counter.
//!
//! `admin::upgrade` bumps `DataKey::ContractVersion`, emits an
//! `("upgrade", version)` audit event carrying the new WASM hash, and then
//! calls `update_current_contract_wasm`. The timelocked upgrade
//! (`TimelockAction::Upgrade`) must behave the same way.
//!
//! The Soroban test host registers contracts defined in the current crate
//! natively (not as Wasm) and allows uploading a zero-byte Wasm blob — the
//! same executable marker native test contracts are registered with. Tests
//! therefore upload a second Wasm and upgrade to it: the full upgrade path
//! (admin auth, version bump, audit event, `update_current_contract_wasm`)
//! runs, while the contract keeps dispatching to the native implementation so
//! `contract_version()` stays callable afterwards.

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::test_utils::decode_events;
use crate::{timelock, TimelockAction};
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short, testutils::{Address as _, Ledger}, Address, BytesN, Env, Symbol, TryIntoVal,
};

/// Zero-byte Wasm blob used as the "second" Wasm to upgrade to.
///
/// The test host explicitly allows uploading an empty Wasm (it is what native
/// test contracts are registered with), so upgrades to it succeed while the
/// contract keeps dispatching to the native implementation.
const SECOND_WASM: &[u8] = &[];

fn upload_second_wasm(env: &Env) -> BytesN<32> {
    env.deployer().upload_contract_wasm(SECOND_WASM)
}

/// Setup: register the contract in `env` and initialize it with `admin` and a
/// signing key. Returns the admin and a client borrowed from `env`.
fn setup(env: &Env) -> (Address, StellarWrapContractClient) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    (admin, client)
}

/// Finds the most recent `("upgrade", version)` event and returns its
/// `(version, wasm_hash)`.
fn last_upgrade_event(env: &Env) -> (u32, BytesN<32>) {
    for (topics, data) in decode_events(env).into_iter().rev() {
        if topics.len() < 2 {
            continue;
        }
        let topic0: Symbol = match topics[0].clone().try_into_val(env) {
            Ok(symbol) => symbol,
            Err(_) => continue,
        };
        if topic0 == symbol_short!("upgrade") {
            let version: u32 = topics[1].clone().try_into_val(env).unwrap();
            let wasm_hash: BytesN<32> = data.try_into_val(env).unwrap();
            return (version, wasm_hash);
        }
    }
    panic!("expected an (\"upgrade\", version) event but none was emitted");
}

#[test]
fn test_contract_version_is_zero_before_any_upgrade() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    assert_eq!(client.contract_version(), 0);
}

#[test]
#[should_panic]
fn test_upgrade_by_non_admin_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);

    // Do NOT mock auths — admin.require_auth() will panic for the caller.
    let wasm_hash = upload_second_wasm(&env);
    client.upgrade(&wasm_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_upgrade_before_initialize_fails_with_not_initialized() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let wasm_hash = upload_second_wasm(&env);
    client.upgrade(&wasm_hash);
}

#[test]
fn test_successful_upgrade_increments_version_by_exactly_one() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    assert_eq!(client.contract_version(), 0);

    let new_wasm_hash = upload_second_wasm(&env);
    client.upgrade(&new_wasm_hash);

    assert_eq!(client.contract_version(), 1);
}

#[test]
fn test_upgrade_event_carries_new_wasm_hash() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let new_wasm_hash = upload_second_wasm(&env);
    client.upgrade(&new_wasm_hash);

    let (version, wasm_hash) = last_upgrade_event(&env);
    assert_eq!(version, 1);
    assert_eq!(wasm_hash, new_wasm_hash);
}

#[test]
fn test_two_successive_upgrades_produce_version_two() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    let first_wasm_hash = upload_second_wasm(&env);
    client.upgrade(&first_wasm_hash);
    assert_eq!(client.contract_version(), 1);

    let second_wasm_hash = upload_second_wasm(&env);
    client.upgrade(&second_wasm_hash);
    assert_eq!(client.contract_version(), 2);
}

#[test]
fn test_timelocked_upgrade_increments_version() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    // Once the timelock is enabled, direct upgrades are blocked, so the
    // upgrade must be scheduled and then executed after the delay.
    client.enable_timelock(&timelock::MIN_DELAY);

    let new_wasm_hash = upload_second_wasm(&env);
    let action = TimelockAction::Upgrade(new_wasm_hash.clone());
    let operation_id = client.timelock_schedule(&action);

    env.ledger().with_mut(|ledger| {
        ledger.timestamp += timelock::MIN_DELAY;
    });
    client.timelock_execute(&operation_id);

    // Read the audit event before any further invocation: the test env only
    // retains the events of the most recent top-level call.
    let (version, wasm_hash) = last_upgrade_event(&env);
    assert_eq!(version, 1);
    assert_eq!(wasm_hash, new_wasm_hash);

    assert_eq!(client.contract_version(), 1);
}
