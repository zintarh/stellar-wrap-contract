#![cfg(test)]

use std::string::ToString;

use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env};
use std::string::ToString;

use crate::{test_utils::sign_payload, StellarWrapContract, StellarWrapContractClient};

#[test]
fn test_has_wrap_agrees_with_get_wrap() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);

    // Before mint: unknown user
    assert!(!client.has_wrap(&user, &period));
    assert_eq!(
        client.has_wrap(&user, &period),
        client.get_wrap(&user, &period).is_some()
    );

    // After mint
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);

    assert!(client.has_wrap(&user, &period));
    assert_eq!(
        client.has_wrap(&user, &period),
        client.get_wrap(&user, &period).is_some()
    );

    // After burn
    client.burn_wrap(&user, &period);

    assert!(!client.has_wrap(&user, &period));
    assert_eq!(
        client.has_wrap(&user, &period),
        client.get_wrap(&user, &period).is_some()
    );
}

#[test]
fn test_version_format() {
    extern crate std;
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Get the version returned by the contract
    let version_str: std::string::String = client.version().to_string();

    // Check if it matches major.minor.patch
    let parts: std::vec::Vec<&str> = version_str.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "Version should have 3 parts separated by dots"
    );

    assert!(
        parts[0].parse::<u32>().is_ok(),
        "Major version must be an integer"
    );
    assert!(
        parts[1].parse::<u32>().is_ok(),
        "Minor version must be an integer"
    );
    assert!(
        parts[2].parse::<u32>().is_ok(),
        "Patch version must be an integer"
    );

    // Make sure it matches the current crate version (if needed)
    assert_eq!(version_str, "0.1.0"); // from Cargo.toml / queries.rs
}

#[test]
fn test_get_admin_pubkey() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Before initialization
    assert!(client.get_admin_pubkey().is_none());

    // After initialization
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    let admin_pubkey = BytesN::from_array(&env, &pubkey_bytes);
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    let returned_pubkey = client.get_admin_pubkey().expect("pubkey should be present");
    assert_eq!(returned_pubkey, admin_pubkey);
}

#[test]
fn test_contract_version() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Before any upgrade
    assert_eq!(client.contract_version(), 0);
}

#[test]
fn test_health() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Before initialize
    let health_before = client.health();
    assert!(!health_before.initialized);
    assert!(!health_before.has_admin);
    assert!(!health_before.has_signing_key);

    // After initialize
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    let health_after = client.health();
    assert!(health_after.initialized);
    assert!(health_after.has_admin);
    assert!(health_after.has_signing_key);
}

// ── get_wrap_summary tests ─────────────────────────────────────────

#[test]
fn test_get_wrap_summary_after_one_mint() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);

    // No wraps yet → None
    assert!(client.get_wrap_summary(&user).is_none());

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);

    let summary = client.get_wrap_summary(&user).expect("summary should exist");
    assert_eq!(summary.total_wraps, 1);
    assert_eq!(summary.periods.len(), 1);
    assert_eq!(summary.periods.get(0), Some(&period));
    assert_eq!(summary.archetypes.len(), 1);
    assert_eq!(summary.archetypes.get(0), Some(&archetype));
    assert_eq!(summary.first_period, period);
    assert_eq!(summary.latest_period, period);
}

#[test]
fn test_get_wrap_summary_after_three_mints() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let hash = BytesN::from_array(&env, &[42u8; 32]);

    let periods = [202401u64, 202402u64, 202403u64];
    let archetypes = [symbol_short!("a"), symbol_short!("b"), symbol_short!("c")];

    for i in 0..3 {
        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            periods[i],
            &archetypes[i],
            &hash,
        );
        client.mint_wrap(&user, &periods[i], &archetypes[i], &hash, &1u32, &signature);
    }

    let summary = client.get_wrap_summary(&user).expect("summary should exist");
    assert_eq!(summary.total_wraps, 3);
    assert_eq!(summary.periods.len(), 3);
    assert_eq!(summary.first_period, 202401);
    assert_eq!(summary.latest_period, 202403);
    assert_eq!(summary.archetypes.len(), 3);
}

#[test]
fn test_get_wrap_summary_after_revoke() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    let hash = BytesN::from_array(&env, &[42u8; 32]);

    // Mint three wraps
    let periods = [202401u64, 202402u64, 202403u64];
    let archetypes = [symbol_short!("a"), symbol_short!("b"), symbol_short!("c")];

    for i in 0..3 {
        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            periods[i],
            &archetypes[i],
            &hash,
        );
        client.mint_wrap(&user, &periods[i], &archetypes[i], &hash, &1u32, &signature);
    }

    // Revoke the middle period
    client.revoke_wrap(&user, &202402u64, &BytesN::from_array(&env, &[0u8; 32]));

    let summary = client.get_wrap_summary(&user).expect("summary should exist");
    assert_eq!(summary.total_wraps, 2);
    assert_eq!(summary.periods.len(), 2);
    assert_eq!(summary.first_period, 202401);
    assert_eq!(summary.latest_period, 202403);
    assert_eq!(summary.archetypes.len(), 2);
    // archetypes should not contain the revoked one
    assert!(!summary.archetypes.iter().any(|a| *a == symbol_short!("b")));

    // Revoke all remaining wraps → None
    client.revoke_wrap(&user, &202401u64, &BytesN::from_array(&env, &[0u8; 32]));
    client.revoke_wrap(&user, &202403u64, &BytesN::from_array(&env, &[0u8; 32]));

    assert!(client.get_wrap_summary(&user).is_none());
}