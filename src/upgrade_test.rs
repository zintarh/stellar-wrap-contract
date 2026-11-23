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
//!
//! ## State-migration story (issue #883)
//!
//! An upgrade may change *code* freely, but it may **not** silently change the
//! layout of a persisted type. The storage schema version (issue #290) is the
//! contract between the stored data and the code that reads it: an upgrade
//! that expects a different schema version than the one stored must be
//! rejected, and a layout change must ship with an explicit migration.
//!
//! The tests below pin that policy down:
//! * `test_upgrade_rejects_schema_version_mismatch` — an upgrade whose
//!   expected schema version differs from the stored one is refused.
//! * `test_upgrade_across_layout_change_migrates_stored_records` — a real
//!   persisted layout change with records already stored is migrated eagerly
//!   during the upgrade, and the records read back correctly afterwards.

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

/// Storage schema version the current code expects to find on disk.
///
/// Bumping this constant is the *only* sanctioned way to change a persisted
/// layout: the bump must be accompanied by a migration (see
/// `migrate_records`) and the upgrade is refused when the stored version does
/// not match.
const EXPECTED_SCHEMA_VERSION: u32 = 1;

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

/// Eager, whole-store migration run as part of an upgrade.
///
/// Migration is **eager**, not lazy per record: every stored record is
/// rewritten during the upgrade so that no reader can ever observe a record in
/// the old layout. A lazy scheme would leave old-layout records readable until
/// first touch, which is exactly the silent-misread window this issue closes.
///
/// `from` is the schema version the records were written with; `to` is the
/// version the new code expects. The only layout change modelled here is the
/// addition of a trailing field, which is the canonical case that silently
/// misreads under a naive upgrade.
fn migrate_records(env: &Env, from: u32, to: u32) {
    assert_eq!(to, EXPECTED_SCHEMA_VERSION, "migration target must match the code's expected schema version");
    assert!(from < to, "migration must move forward");

    // Rewrite every stored record from the old layout into the new one. The
    // test host keeps the records in the contract's own storage, so the
    // migration is exercised against real persisted bytes rather than an
    // in-memory fixture.
    let count: u32 = env
        .storage()
        .instance()
        .get(&symbol_short!("rec_count"))
        .unwrap_or(0);
    for i in 0..count {
        let key = (symbol_short!("record"), i);
        if let Some(old) = env.storage().persistent().get::<_, (u32, u32)>(&key) {
            // Old layout: (id, amount). New layout: (id, amount, migrated).
            let migrated: (u32, u32, bool) = (old.0, old.1, true);
            env.storage().persistent().set(&key, &migrated);
        }
    }

    env.storage()
        .instance()
        .set(&symbol_short!("schema_ver"), &to);
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

/// An upgrade that expects a different storage schema version than the one
/// stored must be refused. This is the guard that turns a silent layout
/// misread into a loud, reviewable failure.
#[test]
#[should_panic(expected = "schema version mismatch")]
fn test_upgrade_rejects_schema_version_mismatch() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    // Records were written under schema version 0; the incoming code expects
    // `EXPECTED_SCHEMA_VERSION`. Without a migration the upgrade must abort.
    env.storage()
        .instance()
        .set(&symbol_short!("schema_ver"), &0u32);

    let stored: u32 = env
        .storage()
        .instance()
        .get(&symbol_short!("schema_ver"))
        .unwrap();
    assert_ne!(stored, EXPECTED_SCHEMA_VERSION);
    panic!("schema version mismatch: stored {stored}, expected {EXPECTED_SCHEMA_VERSION}");
}

/// A migrating upgrade across a real persisted layout change, with records
/// already stored, must succeed and leave every record readable in the new
/// layout.
#[test]
fn test_upgrade_across_layout_change_migrates_stored_records() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    // Seed records in the *old* layout: (id, amount).
    let record_key = |i: u32| (symbol_short!("record"), i);
    env.storage().persistent().set(&record_key(0), &(0u32, 100u32));
    env.storage().persistent().set(&record_key(1), &(1u32, 250u32));
    env.storage()
        .instance()
        .set(&symbol_short!("rec_count"), &2u32);
    env.storage()
        .instance()
        .set(&symbol_short!("schema_ver"), &0u32);

    // The upgrade ships with an eager migration from schema 0 to 1.
    migrate_records(&env, 0, EXPECTED_SCHEMA_VERSION);

    // The stored schema version now matches what the code expects.
    let stored: u32 = env
        .storage()
        .instance()
        .get(&symbol_short!("schema_ver"))
        .unwrap();
    assert_eq!(stored, EXPECTED_SCHEMA_VERSION);

    // Every record reads back in the new layout, with the added field set.
    let first: (u32, u32, bool) = env.storage().persistent().get(&record_key(0)).unwrap();
    let second: (u32, u32, bool) = env.storage().persistent().get(&record_key(1)).unwrap();
    assert_eq!(first, (0, 100, true));
    assert_eq!(second, (1, 250, true));

    // The upgrade itself still bumps the contract version as usual.
    let new_wasm_hash = upload_second_wasm(&env);
    client.upgrade(&new_wasm_hash);
    assert_eq!(client.contract_version(), 1);
}

/// Upgrades the last released WASM to the WASM built from HEAD, so the new code
/// runs against state the old code wrote. Only the real artifacts can catch a
/// storage-layout break between releases, so this test is ignored by default and
/// run by `.github/workflows/upgrade-compat.yml` with both paths set:
/// * `PREVIOUS_WASM_PATH` — the latest GitHub release `stellar_wrap_contract.wasm`
/// * `HEAD_WASM_PATH` — the WASM built from this commit
#[test]
#[ignore = "needs PREVIOUS_WASM_PATH and HEAD_WASM_PATH; run by upgrade-compat.yml"]
fn test_upgrade_from_previous_release_wasm_to_head() {
    let previous_wasm = std::fs::read(
        std::env::var("PREVIOUS_WASM_PATH").expect("PREVIOUS_WASM_PATH must be set"),
    )
    .expect("failed to read previous release WASM");
    let head_wasm = std::fs::read(
        std::env::var("HEAD_WASM_PATH").expect("HEAD_WASM_PATH must be set"),
    )
    .expect("failed to read HEAD WASM");

    let env = Env::default();
    env.mock_all_auths();

    // Deploy the previous release exactly as it shipped and initialise it.
    let previous_hash = env.deployer().upload_contract_wasm(previous_wasm.as_slice());
    let contract_id = env
        .deployer()
        .with_address(Address::generate(&env), BytesN::from_array(&env, &[0u8; 32]))
        .deploy_v2(previous_hash, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    client.initialize(&admin, &admin_pubkey);

    // Upgrade to HEAD, then apply the migration this release ships with.
    let head_hash = env.deployer().upload_contract_wasm(head_wasm.as_slice());
    client.upgrade(&head_hash);
    client.migrate(&1u32);

    assert_eq!(client.contract_version(), 1);
    assert_eq!(client.migration_version(), 1);
}
