#![cfg(test)]

use crate::{StellarWrapContract, StellarWrapContractClient};
use crate::test_utils::sign_payload;
use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env};

#[test]
fn test_has_wrap_agrees_with_get_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);

    // Before mint: unknown user
    assert!(!client.has_wrap(&user, &period));
    assert_eq!(client.has_wrap(&user, &period), client.get_wrap(&user, &period).is_some());

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
    assert_eq!(client.has_wrap(&user, &period), client.get_wrap(&user, &period).is_some());

    // After burn
    client.burn_wrap(&user, &period);

    assert!(!client.has_wrap(&user, &period));
    assert_eq!(client.has_wrap(&user, &period), client.get_wrap(&user, &period).is_some());
}

#[test]
fn test_version_format() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Get the version returned by the contract
    let version_str: alloc::string::String = client.version().into();
    
    // Check if it matches major.minor.patch
    let parts: std::vec::Vec<&str> = version_str.split('.').collect();
    assert_eq!(parts.len(), 3, "Version should have 3 parts separated by dots");
    
    assert!(parts[0].parse::<u32>().is_ok(), "Major version must be an integer");
    assert!(parts[1].parse::<u32>().is_ok(), "Minor version must be an integer");
    assert!(parts[2].parse::<u32>().is_ok(), "Patch version must be an integer");
    
    // Make sure it matches the current crate version (if needed)
    assert_eq!(version_str.as_str(), "0.1.0"); // from Cargo.toml / queries.rs
}

#[test]
fn test_get_admin_pubkey() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
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
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Before any upgrade
    assert_eq!(client.contract_version(), 0);
}

#[test]
fn test_health() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
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
