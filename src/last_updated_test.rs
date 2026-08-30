#![cfg(test)]

extern crate std;

use super::*;
use crate::test_utils::sign_payload;
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

/// Helper: mint one wrap for `user` at the current ledger time using the
/// contract admin key.
fn mint_wrap_at_current_time(
    env: &Env,
    client: &StellarWrapContractClient,
    signing_key: &SigningKey,
    contract_id: &Address,
    user: &Address,
    period: u64,
) {
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(env, &[7u8; 32]);
    let signature = sign_payload(
        env,
        signing_key,
        contract_id,
        user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(user, &period, &archetype, &data_hash, &1u32, &signature);
}

fn setup(env: &Env, client: &StellarWrapContractClient) -> (SigningKey, Address, Address) {
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let user = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    (signing_key, admin, user)
}

#[test]
fn test_last_updated_none_before_any_action() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let (_, _, user) = setup(&env, &client);

    assert_eq!(client.get_last_updated(&user), None);
}

#[test]
fn test_last_updated_after_mint_matches_ledger_time() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let (signing_key, _, user) = setup(&env, &client);

    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user, 202401);

    assert_eq!(client.get_last_updated(&user), Some(1_000_000));
}

#[test]
fn test_last_updated_monotonic_across_mints() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let (signing_key, _, user) = setup(&env, &client);

    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user, 202401);
    assert_eq!(client.get_last_updated(&user), Some(1_000_000));

    // Advance the ledger; a new mint must move the marker forward.
    env.ledger().with_mut(|li| li.timestamp = 1_000_001);
    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user, 202402);
    assert_eq!(client.get_last_updated(&user), Some(1_000_001));

    // Advance again; still strictly monotonic.
    env.ledger().with_mut(|li| li.timestamp = 1_000_050);
    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user, 202403);
    assert_eq!(client.get_last_updated(&user), Some(1_000_050));
}

#[test]
fn test_last_updated_updated_on_revoke() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let (signing_key, _, user) = setup(&env, &client);

    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user, 202401);
    assert_eq!(client.get_last_updated(&user), Some(1_000_000));

    // Advance the ledger and revoke: the marker must move forward again.
    env.ledger().with_mut(|li| li.timestamp = 1_000_100);
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user, &202401, &reason);
    assert_eq!(client.get_last_updated(&user), Some(1_000_100));
}

#[test]
fn test_last_updated_independent_per_user() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let (signing_key, _, user_a) = setup(&env, &client);
    let user_b = Address::generate(&env);

    mint_wrap_at_current_time(&env, &client, &signing_key, &contract_id, &user_a, 202401);

    assert_eq!(client.get_last_updated(&user_a), Some(1_000_000));
    assert_eq!(client.get_last_updated(&user_b), None);
}
