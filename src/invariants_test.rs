#![cfg(test)]

use crate::{
    bridge_test::{setup_single_relayer_bridge_in, setup_test_env, sign_mint_payload},
    mint::CURRENT_PAYLOAD_VERSION,
    StellarWrapContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol};

fn mint(env: &Env, client: &StellarWrapContractClient, key: &ed25519_dalek::SigningKey, user: &Address, period: u64) {
    let archetype = Symbol::new(env, "arch");
    let data_hash = BytesN::from_array(env, &[1; 32]);
    let signature = sign_mint_payload(env, key, &client.address, user, period, &archetype, &data_hash);
    client.mint_wrap(user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &signature);
}

#[test]
fn test_healthy_user_invariants() {
    let env = Env::default();
    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);

    // Check invariants before any action
    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
    assert!(report.latest_period_matches_max);
    assert!(report.all_user_periods_live);
    assert!(report.balance_matches_wrap_count);
    assert_eq!(report.wrap_count, 0);

    mint(&env, &client, &signing_key, &user, 202401);

    // Check invariants again
    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
    assert!(report.latest_period_matches_max);
    assert!(report.all_user_periods_live);
    assert!(report.balance_matches_wrap_count);
    assert_eq!(report.wrap_count, 1);
    assert_eq!(report.latest_period, Some(202401));
}

#[test]
fn test_post_revoke_invariants() {
    let env = Env::default();
    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);

    mint(&env, &client, &signing_key, &user, 202401);
    client.revoke_wrap(&user, &202401, &BytesN::from_array(&env, &[9; 32]));

    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
}

#[test]
fn test_post_burn_invariants() {
    let env = Env::default();
    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);

    mint(&env, &client, &signing_key, &user, 202401);
    client.burn_wrap(&user, &202401);

    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
}

#[test]
fn test_post_bridge_in_invariants() {
    let env = Env::default();
    let (client, _admin, relayer_key, _signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let archetype = Symbol::new(&env, "arch");
    let data_hash = BytesN::from_array(&env, &[1; 32]);

    setup_single_relayer_bridge_in(&env, &client, &relayer_key, 1, 1, &user, 202402, &archetype, &data_hash);

    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
    assert!(report.latest_period_matches_max);
    assert!(report.all_user_periods_live);
    assert!(report.balance_matches_wrap_count);
    assert_eq!(report.wrap_count, 1);
}
