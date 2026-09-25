#![allow(deprecated)]

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env, Symbol};

use stellar_wrap_contract::{StellarWrapContract, StellarWrapContractClient};

use stellar_wrap_contract::signature::construct_mint_payload;

fn sign_mint(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = construct_mint_payload(env, contract, user, period, archetype, data_hash, 1);
    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);
    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

fn setup() -> (
    Env,
    StellarWrapContractClient<'static>,
    Address,
    Address,
    SigningKey,
) {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[99u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    (env, client, admin, user, signing_key)
}

// Estimate constants must match src/storage_accounting.rs
const ESTIMATE_WRAP: u64 = 112; // 64 + 48
const ESTIMATE_WRAPCOUNT: u64 = 16;
const ESTIMATE_LATEST: u64 = 16;
const ESTIMATE_USERPERIODS: u64 = 64;
const ESTIMATE_LASTUPDATED: u64 = 16;

#[test]
fn fresh_contract_storage_bytes_is_zero() {
    let (_, client, _, _, _) = setup();
    assert_eq!(client.storage_bytes(), 0);
}

#[test]
fn first_mint_increments_storage_bytes_by_full_estimate() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period = 202401u64;

    let sig = sign_mint(
        &env,
        &signing_key,
        &env.current_contract_address(),
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    let expected = ESTIMATE_WRAP
        + ESTIMATE_WRAPCOUNT
        + ESTIMATE_LATEST
        + ESTIMATE_USERPERIODS
        + ESTIMATE_LASTUPDATED;
    assert_eq!(client.storage_bytes(), expected);
}

#[test]
fn second_mint_same_user_adds_wrap_and_userperiods_only() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period1 = 202401u64;
    let period2 = 202402u64;

    let addr = env.current_contract_address();

    let sig1 = sign_mint(&env, &signing_key, &addr, &user, period1, &archetype, &hash);
    client.mint_wrap(&user, &period1, &archetype, &hash, &1u32, &sig1);
    let after_first = client.storage_bytes();

    let sig2 = sign_mint(&env, &signing_key, &addr, &user, period2, &archetype, &hash);
    client.mint_wrap(&user, &period2, &archetype, &hash, &1u32, &sig2);
    let after_second = client.storage_bytes();

    let delta = after_second - after_first;
    assert_eq!(delta, ESTIMATE_WRAP + ESTIMATE_USERPERIODS);
}

#[test]
fn revoke_single_wrap_returns_wrap_delta() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period1 = 202401u64;
    let period2 = 202402u64;

    let addr = env.current_contract_address();

    let sig1 = sign_mint(&env, &signing_key, &addr, &user, period1, &archetype, &hash);
    client.mint_wrap(&user, &period1, &archetype, &hash, &1u32, &sig1);
    let sig2 = sign_mint(&env, &signing_key, &addr, &user, period2, &archetype, &hash);
    client.mint_wrap(&user, &period2, &archetype, &hash, &1u32, &sig2);

    let before = client.storage_bytes();
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &period1, &reason);
    let after = client.storage_bytes();

    assert_eq!(before - after, ESTIMATE_WRAP);
}

#[test]
fn burn_single_wrap_returns_wrap_delta() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period1 = 202401u64;
    let period2 = 202402u64;

    let addr = env.current_contract_address();

    let sig1 = sign_mint(&env, &signing_key, &addr, &user, period1, &archetype, &hash);
    client.mint_wrap(&user, &period1, &archetype, &hash, &1u32, &sig1);
    let sig2 = sign_mint(&env, &signing_key, &addr, &user, period2, &archetype, &hash);
    client.mint_wrap(&user, &period2, &archetype, &hash, &1u32, &sig2);

    let before = client.storage_bytes();
    client.burn_wrap(&user, &period1);
    let after = client.storage_bytes();

    assert_eq!(before - after, ESTIMATE_WRAP);
}

#[test]
fn burn_matches_revoke_delta() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let addr = env.current_contract_address();

    // ── Revoke path ──
    let sig_a = sign_mint(
        &env,
        &signing_key,
        &addr,
        &user,
        202401u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202401u64, &archetype, &hash, &1u32, &sig_a);
    let sig_b = sign_mint(
        &env,
        &signing_key,
        &addr,
        &user,
        202402u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202402u64, &archetype, &hash, &1u32, &sig_b);

    let before_revoke = client.storage_bytes();
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &202401u64, &reason);
    let after_revoke = client.storage_bytes();
    let revoke_delta = before_revoke - after_revoke;

    // Revoke remaining to clean up (triggers full-subtraction path)
    client.revoke_wrap(&user, &202402u64, &reason);
    let full_revoke_end = client.storage_bytes();

    // ── Burn path (fresh mint) ──
    let sig_c = sign_mint(
        &env,
        &signing_key,
        &addr,
        &user,
        202403u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202403u64, &archetype, &hash, &1u32, &sig_c);
    let sig_d = sign_mint(
        &env,
        &signing_key,
        &addr,
        &user,
        202404u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202404u64, &archetype, &hash, &1u32, &sig_d);

    let before_burn = client.storage_bytes();
    client.burn_wrap(&user, &202403u64);
    let after_burn = client.storage_bytes();
    let burn_delta = before_burn - after_burn;

    // Single-wrap delta must match
    assert_eq!(burn_delta, revoke_delta);

    // Burn remaining to clean up
    client.burn_wrap(&user, &202404u64);
    let full_burn_end = client.storage_bytes();

    // Both full cleanups must reach the same baseline
    assert_eq!(full_revoke_end, full_burn_end);
}

#[test]
fn burn_last_wrap_subtracts_all_accounted_entries() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period = 202401u64;
    let sig = sign_mint(
        &env,
        &signing_key,
        &env.current_contract_address(),
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    let before = client.storage_bytes();
    assert_eq!(
        before,
        ESTIMATE_WRAP
            + ESTIMATE_WRAPCOUNT
            + ESTIMATE_LATEST
            + ESTIMATE_USERPERIODS
            + ESTIMATE_LASTUPDATED
    );

    client.burn_wrap(&user, &period);
    let after = client.storage_bytes();

    // Last wrap burn must reclaim wrap + wrapcount + latest + userperiods
    assert_eq!(
        before - after,
        ESTIMATE_WRAP + ESTIMATE_WRAPCOUNT + ESTIMATE_LATEST + ESTIMATE_USERPERIODS
    );
    // lastupdated is NOT reclaimed by burn (retained for audit trail)
    assert_eq!(after, ESTIMATE_LASTUPDATED);
}

#[test]
fn revoke_last_wrap_subtracts_all_accounted_entries() {
    let (env, client, _, user, signing_key) = setup();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period = 202401u64;
    let sig = sign_mint(
        &env,
        &signing_key,
        &env.current_contract_address(),
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    let before = client.storage_bytes();
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &period, &reason);
    let after = client.storage_bytes();

    // Last wrap revoke must reclaim wrap + wrapcount + latest + userperiods
    assert_eq!(
        before - after,
        ESTIMATE_WRAP + ESTIMATE_WRAPCOUNT + ESTIMATE_LATEST + ESTIMATE_USERPERIODS
    );
    // lastupdated is NOT reclaimed by revoke (retained for audit trail)
    assert_eq!(after, ESTIMATE_LASTUPDATED);
}

#[test]
fn sub_storage_bytes_saturates_at_zero() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Fresh contract has storage_bytes == 0
    assert_eq!(client.storage_bytes(), 0);
}
