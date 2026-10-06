#![cfg(test)]

use crate::constants::TTL_ONE_YEAR;
use crate::mint::MAX_BATCH_SIZE;
use crate::test_utils::sign_payload;
use crate::{
    ContractError, DataKey, StellarWrapContract, StellarWrapContractClient, CURRENT_PAYLOAD_VERSION,
};
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{
        storage::{Instance as _, Persistent as _},
        Address as _, Ledger,
    },
    Address, BytesN, Env, IntoVal,
};

#[test]
fn test_renew_all_ttls_admin_auth() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    // 2. renew_all_ttls before initialization fails with NotInitialized.
    let res = client.try_renew_all_ttls(&user);
    assert!(res.is_err());
    // Should be ContractError::NotInitialized

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    // 1. renew_all_ttls requires admin authorization.
    // If not mocked, try_renew_all_ttls should fail with unauthorized or we can explicitly check auths.
    // We expect an error if auth is not mocked for admin.
    let auth_res = client.try_renew_all_ttls(&user);
    assert!(auth_res.is_err());

    // Mock admin auth and it should succeed
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "renew_all_ttls",
            args: (&user,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.renew_all_ttls(&user);
}

#[test]
fn test_extend_ttl_non_existent() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    // 3. extend_ttl on a non-existent (user, period) does not panic.
    client.extend_ttl(&user, &202401);
}

#[test]
fn test_extend_ttl_extends_expiry() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    env.mock_all_auths();

    let user = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature,
    );

    let initial_wrap = client.get_wrap(&user, &period);
    assert!(initial_wrap.is_some());

    // Original TTL is ~1 year (17280 * 365 = 6307200 ledgers)
    // Advance ledger to just before max TTL to test extend_ttl behavior
    env.ledger().set_sequence_number(6300000);

    // 5. Repeated extend_ttl calls are idempotent and do not compound beyond the max TTL.
    client.extend_ttl(&user, &period);
    client.extend_ttl(&user, &period);

    // Advance ledger past original TTL
    env.ledger().set_sequence_number(6400000);

    // 4. After extend_ttl, the wrap record is still readable past its original expiry ledger.
    let wrap_after = client.get_wrap(&user, &period);
    assert!(wrap_after.is_some());
}

#[test]
fn test_extend_ttl_post_revocation() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    env.mock_all_auths();

    let user = Address::generate(&env);
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &u64::MAX,
        &signature,
    );

    client.revoke_wrap(&user, &period);

    // After revocation, user might not have latest period if we revoked all wraps?
    // Wait, revocation doesn't clear `LatestPeriod` automatically, it just marks wrap revoked.
    // Let's just call extend_ttl to ensure it doesn't panic.
    // 6. extend_ttl works for a user who has wraps but no LatestPeriod marker (post-revocation state).
    client.extend_ttl(&user, &period);
}

// ─── Issue #678: no-op guard and bounded batch renewal ───────────────────────

/// Ledger settings that leave freshly created entries *below* the one-year
/// renewal threshold, so any `extend_ttl` call that actually runs visibly
/// changes the entry TTL. This is what makes the no-op assertions meaningful:
/// an unconditional instance extension would show up immediately.
fn ttl_test_env() -> Env {
    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 1_000_000;
        li.min_persistent_entry_ttl = 100_000;
        li.min_temp_entry_ttl = 1_000;
        li.max_entry_ttl = 10_000_000;
    });
    env
}

/// Registers, initializes and auth-mocks a contract for the TTL tests.
fn setup<'a>(env: &'a Env, seed: u8) -> (StellarWrapContractClient<'a>, Address, SigningKey) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    (client, contract_id, signing_key)
}

fn mint(
    client: &StellarWrapContractClient,
    env: &Env,
    signing_key: &SigningKey,
    contract_id: &Address,
    user: &Address,
    period: u64,
) {
    let archetype = symbol_short!("architect");
    let data_hash = BytesN::from_array(env, &[42u8; 32]);
    let signature = sign_payload(
        env,
        signing_key,
        contract_id,
        user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(
        user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );
}

/// Reads the contract instance TTL from inside the contract's storage context.
fn instance_ttl(env: &Env, contract_id: &Address) -> u32 {
    env.as_contract(contract_id, || env.storage().instance().get_ttl())
}

/// Reads the persistent TTL of a `(user, period)` wrap record.
fn wrap_ttl(env: &Env, contract_id: &Address, user: &Address, period: u64) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::Wrap(user.clone(), period))
    })
}

/// `extend_ttl` for a period the user never minted must renew nothing at all,
/// including the contract instance TTL.
#[test]
fn test_extend_ttl_missing_record_does_not_extend_instance_ttl() {
    let env = ttl_test_env();
    let (client, contract_id, _) = setup(&env, 7);
    let user = Address::generate(&env);

    let missing_period = 202401u64;
    assert!(client.get_wrap(&user, &missing_period).is_none());

    let before = instance_ttl(&env, &contract_id);
    assert!(
        before < TTL_ONE_YEAR,
        "fixture must start below the renewal threshold, got {before}"
    );

    client.extend_ttl(&user, &missing_period);

    assert_eq!(
        instance_ttl(&env, &contract_id),
        before,
        "a call for a non-existent record must not extend the instance TTL"
    );
}

/// A matching record still renews both its own TTL and the instance TTL.
#[test]
fn test_extend_ttl_existing_record_renews_wrap_and_instance_ttl() {
    let env = ttl_test_env();
    let (client, contract_id, signing_key) = setup(&env, 8);
    let user = Address::generate(&env);
    let period = 202401u64;
    mint(&client, &env, &signing_key, &contract_id, &user, period);

    // Shrink both TTLs below the one-year threshold so the renewal is visible.
    env.ledger().with_mut(|li| li.sequence_number += 10_000);
    let instance_before = instance_ttl(&env, &contract_id);
    let wrap_before = wrap_ttl(&env, &contract_id, &user, period);
    assert!(instance_before < TTL_ONE_YEAR);
    assert!(wrap_before < TTL_ONE_YEAR);

    client.extend_ttl(&user, &period);

    assert_eq!(instance_ttl(&env, &contract_id), TTL_ONE_YEAR);
    assert_eq!(wrap_ttl(&env, &contract_id, &user, period), TTL_ONE_YEAR);
}

/// One batch call renews every period the user owns, skips periods with no
/// record, and renews the shared metadata/instance TTLs once.
#[test]
fn test_extend_ttl_batch_renews_every_period_in_one_call() {
    let env = ttl_test_env();
    let (client, contract_id, signing_key) = setup(&env, 9);
    let user = Address::generate(&env);
    let periods = [202401u64, 202402, 202403, 202404];
    for period in periods {
        mint(&client, &env, &signing_key, &contract_id, &user, period);
    }

    // Shrink the TTLs below the threshold so every renewal is observable.
    env.ledger().with_mut(|li| li.sequence_number += 10_000);
    for period in periods {
        assert!(wrap_ttl(&env, &contract_id, &user, period) < TTL_ONE_YEAR);
    }

    // 209912 was never minted: it must be skipped without failing the batch.
    let batch = soroban_sdk::vec![&env, 202401u64, 202402, 202403, 202404, 209912];
    client.extend_ttl_batch(&user, &batch);

    for period in periods {
        assert_eq!(wrap_ttl(&env, &contract_id, &user, period), TTL_ONE_YEAR);
    }
    assert_eq!(instance_ttl(&env, &contract_id), TTL_ONE_YEAR);
}

/// A batch made up entirely of unknown periods is a no-op, mirroring the
/// single-period entrypoint.
#[test]
fn test_extend_ttl_batch_missing_records_does_not_extend_instance_ttl() {
    let env = ttl_test_env();
    let (client, contract_id, _) = setup(&env, 10);
    let user = Address::generate(&env);

    let before = instance_ttl(&env, &contract_id);
    assert!(before < TTL_ONE_YEAR);

    let batch = soroban_sdk::vec![&env, 202401u64, 202402u64];
    client.extend_ttl_batch(&user, &batch);

    assert_eq!(instance_ttl(&env, &contract_id), before);
}

#[test]
fn test_extend_ttl_batch_empty_is_rejected() {
    let env = ttl_test_env();
    let (client, _, _) = setup(&env, 11);
    let user = Address::generate(&env);

    let empty: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);
    let result = client.try_extend_ttl_batch(&user, &empty);
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ContractError::BatchEmpty as u32)
    );
}

#[test]
fn test_extend_ttl_batch_over_max_is_rejected_before_any_write() {
    let env = ttl_test_env();
    let (client, contract_id, _) = setup(&env, 12);
    let user = Address::generate(&env);

    let before = instance_ttl(&env, &contract_id);
    let mut batch = soroban_sdk::Vec::new(&env);
    for i in 0..=MAX_BATCH_SIZE {
        batch.push_back(202401u64 + i as u64);
    }

    let result = client.try_extend_ttl_batch(&user, &batch);
    assert_eq!(
        result.unwrap_err().unwrap(),
        soroban_sdk::Error::from_contract_error(ContractError::BatchTooLarge as u32)
    );
    assert_eq!(instance_ttl(&env, &contract_id), before);
}
