#![cfg(test)]

use crate::{StellarWrapContract, StellarWrapContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol};

#[test]
fn test_healthy_user_invariants() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Check invariants before any action
    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
    assert!(report.latest_period_matches_max);
    assert!(report.all_user_periods_live);
    assert!(report.balance_matches_wrap_count);
    assert_eq!(report.wrap_count, 0);

    // Mint a wrap
    let archetype = Symbol::new(&env, "arch");
    let data_hash = BytesN::from_array(&env, &[0; 32]);
    let signature = BytesN::from_array(&env, &[0; 64]);
    client.mint_wrap(&user, &202401, &archetype, &data_hash, &1u32, &signature);

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
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let archetype = Symbol::new(&env, "arch");
    let data_hash = BytesN::from_array(&env, &[0; 32]);
    let signature = BytesN::from_array(&env, &[0; 64]);
    client.mint_wrap(&user, &202401, &archetype, &data_hash, &1u32, &signature);

    // Revoke
    client.revoke_wrap(&user, &202401, &BytesN::from_array(&env, &[0; 32]));

    // Check invariants
    let report = client.check_user_invariants(&user);
    // Usually revoke removes the wrap but leaves the period? Or removes period?
    // Depending on contract logic, it should maintain invariants.
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
    // Either all valid, or some specific invariant logic applies.
}

#[test]
fn test_post_burn_invariants() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let archetype = Symbol::new(&env, "arch");
    let data_hash = BytesN::from_array(&env, &[0; 32]);
    let signature = BytesN::from_array(&env, &[0; 64]);
    client.mint_wrap(&user, &202401, &archetype, &data_hash, &1u32, &signature);

    // Burn
    client.burn_wrap(&user, &202401);

    // Check invariants
    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_match_wrap_periods);
}

#[test]
fn test_post_bridge_in_invariants() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Enable bridge chain
    client.set_chain_status(&1, &true);
    // Allow bridge in
    let archetype = Symbol::new(&env, "arch");
    let data_hash = BytesN::from_array(&env, &[0; 32]);
    
    // The admin must be relayer or mock auth handles it
    client.set_bridge_relayer(&admin);
    
    client.bridge_wrap_in(&1, &1, &user, &202402, &archetype, &data_hash);

    let report = client.check_user_invariants(&user);
    assert!(report.wrap_count_match_user_periods);
    assert!(report.wrap_count_matches_wrap_periods);
    assert!(report.latest_period_matches_max);
    assert!(report.all_user_periods_live);
    assert!(report.balance_matches_wrap_count);
    assert_eq!(report.wrap_count, 1);
}
