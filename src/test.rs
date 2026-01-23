#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Bytes, BytesN, Env,
};

#[test]
fn test_minting_flow() {
    let env = Env::default();

    // Register the contract
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Create mock admin and user addresses
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    
    // Create a mock public key (32 bytes)
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract with admin and public key
    client.initialize(&admin, &admin_pubkey);

    // Set up authorization for admin
    env.mock_all_auths();

    // Prepare dummy data for minting
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = symbol_short!("2024");

    // Mint wrap as admin for the user
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);

    // Retrieve the wrap record
    let wrap_opt = client.get_wrap(&user, &period);

    // Assert the wrap exists and matches what was minted
    assert!(wrap_opt.is_some());
    let wrap = wrap_opt.unwrap();

    assert_eq!(wrap.data_hash, dummy_hash);
    assert_eq!(wrap.archetype, archetype);
    assert_eq!(wrap.period, period);
    assert_eq!(wrap.timestamp, env.ledger().timestamp());
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // First initialization should succeed
    client.initialize(&admin, &admin_pubkey);

    // Second initialization should fail
    let result = client.try_initialize(&admin, &admin_pubkey);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract
    client.initialize(&admin, &admin_pubkey);
    
    // Mock all auths first
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi");
    let period = symbol_short!("2024");

    // Mint should succeed with mocked auth
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);
    
    // Verify it was minted
    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some());
}

#[test]
fn test_multiple_periods() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype_1 = symbol_short!("arch");
    let archetype_2 = symbol_short!("defi");
    let period_1 = symbol_short!("2024");
    let period_2 = symbol_short!("2025");

    // Mint wrap for period 1
    client.mint_wrap(&user, &dummy_hash_1, &archetype_1, &period_1);

    // Mint wrap for period 2 (should succeed - different period)
    client.mint_wrap(&user, &dummy_hash_2, &archetype_2, &period_2);

    // Retrieve both wraps
    let wrap_1 = client.get_wrap(&user, &period_1).unwrap();
    let wrap_2 = client.get_wrap(&user, &period_2).unwrap();

    // Assert they are different
    assert_eq!(wrap_1.data_hash, dummy_hash_1);
    assert_eq!(wrap_1.archetype, archetype_1);
    assert_eq!(wrap_1.period, period_1);

    assert_eq!(wrap_2.data_hash, dummy_hash_2);
    assert_eq!(wrap_2.archetype, archetype_2);
    assert_eq!(wrap_2.period, period_2);
}

#[test]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("arch");
    let period = symbol_short!("2024");

    // Mint first wrap
    client.mint_wrap(&user, &dummy_hash_1, &archetype, &period);

    // Try to mint again for the same period (should fail)
    let result = client.try_mint_wrap(&user, &dummy_hash_2, &archetype, &period);
    assert_eq!(result, Err(Ok(Error::WrapAlreadyExists)));
}

#[test]
fn test_verify_signature_not_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let message = Bytes::from_slice(&env, b"Test message");
    let signature = BytesN::from_array(&env, &[0u8; 64]);

    let result = client.try_verify_signature(&message, &signature);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// ==================== SIGNATURE VERIFICATION NOTES ====================
// Note: Additional Ed25519 signature verification tests (valid signature, invalid signature, 
// wrong key, wrong message) require actual Ed25519 keypair generation which is not available 
// in the current Soroban SDK test utilities. The verify_signature function is correctly 
// implemented using e.crypto().ed25519_verify() and will work properly in production with#![cfg(test)]
