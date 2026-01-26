#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as TestAddress, Bytes, BytesN, Env};

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
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01"); // January 2024

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
#[should_panic(expected = "Unauthorized")]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract
    client.initialize(&admin, &admin_pubkey);

    // Do not mock auths - should fail with unauthorized

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi");
    let period = symbol_short!("2024_01");

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
    let archetype_1 = symbol_short!("soroban");
    let archetype_2 = symbol_short!("defi");
    let period_1 = symbol_short!("2024_01"); // January
    let period_2 = symbol_short!("2024_02"); // February

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
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01");

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

#[test]
fn test_update_admin_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Create a mock public key (32 bytes)
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract with admin
    client.initialize(&admin, &admin_pubkey);

    // Set up authorization for admin
    env.mock_all_auths();

    // Update admin (should succeed)
    client.update_admin(&new_admin);

    // Verify new admin can mint (proving the update worked)
    let user = Address::generate(&env);
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01");

    // This should succeed because new_admin is now the admin
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);

    // Verify the wrap was created
    let wrap_opt = client.get_wrap(&user, &period);
    assert!(wrap_opt.is_some());
}

#[test]
#[should_panic]
fn test_update_admin_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let _unauthorized = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // Create a mock public key (32 bytes)
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract
    client.initialize(&admin, &admin_pubkey);

    // Don't set up mock_all_auths - this means require_auth will fail
    // Try to update admin as unauthorized user (should fail with Auth error)
    client.update_admin(&new_admin);
}

// ============================================================================
// Query Function Tests
// ============================================================================

#[test]
fn test_get_wrap_existing() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01");

    // Mint a wrap
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);

    // Query the wrap - should return Some
    let wrap_opt = client.get_wrap(&user, &period);
    assert!(wrap_opt.is_some());

    let wrap = wrap_opt.unwrap();
    assert_eq!(wrap.data_hash, dummy_hash);
    assert_eq!(wrap.archetype, archetype);
    assert_eq!(wrap.period, period);
}

#[test]
fn test_get_wrap_nonexistent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);

    use soroban_sdk::symbol_short;
    let period = symbol_short!("2024_01");

    // Query a wrap that doesn't exist - should return None
    let wrap_opt = client.get_wrap(&user, &period);
    assert!(wrap_opt.is_none());
}

#[test]
fn test_get_wrap_different_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user1 = <Address as TestAddress>::generate(&env);
    let user2 = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01");

    // Mint wrap for user1
    client.mint_wrap(&user1, &dummy_hash, &archetype, &period);

    // Query for user2 - should return None
    let wrap_opt = client.get_wrap(&user2, &period);
    assert!(wrap_opt.is_none());
}

#[test]
fn test_get_count_with_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let dummy_hash_3 = BytesN::from_array(&env, &[123u8; 32]);
    let archetype = symbol_short!("soroban");

    // Initially count should be 0
    assert_eq!(client.get_count(&user), 0);

    // Mint first wrap
    client.mint_wrap(&user, &dummy_hash_1, &archetype, &symbol_short!("2024_01"));
    assert_eq!(client.get_count(&user), 1);

    // Mint second wrap
    client.mint_wrap(&user, &dummy_hash_2, &archetype, &symbol_short!("2024_02"));
    assert_eq!(client.get_count(&user), 2);

    // Mint third wrap
    client.mint_wrap(&user, &dummy_hash_3, &archetype, &symbol_short!("2024_03"));
    assert_eq!(client.get_count(&user), 3);
}

#[test]
fn test_get_count_no_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);

    // Query count for user with no wraps - should return 0
    assert_eq!(client.get_count(&user), 0);
}

#[test]
fn test_get_count_multiple_users() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user1 = <Address as TestAddress>::generate(&env);
    let user2 = <Address as TestAddress>::generate(&env);

    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");

    // Mint wraps for user1
    client.mint_wrap(&user1, &dummy_hash, &archetype, &symbol_short!("2024_01"));
    client.mint_wrap(&user1, &dummy_hash, &archetype, &symbol_short!("2024_02"));

    // Mint wrap for user2
    client.mint_wrap(&user2, &dummy_hash, &archetype, &symbol_short!("2024_01"));

    // Verify counts are independent
    assert_eq!(client.get_count(&user1), 2);
    assert_eq!(client.get_count(&user2), 1);
}

#[test]
fn test_get_admin_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);

    // Initialize contract
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);

    // Query admin - should return Some with the admin address
    let admin_opt = client.get_admin();
    assert!(admin_opt.is_some());
    assert_eq!(admin_opt.unwrap(), admin);
}

#[test]
fn test_get_admin_not_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Query admin without initializing - should return None
    let admin_opt = client.get_admin();
    assert!(admin_opt.is_none());
}

// ========== State Verification Tests (Manual Storage Injection) ==========

#[test]
fn test_get_wrap_state_verification() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);

    // Manually inject state into storage
    use soroban_sdk::symbol_short;
    use storage_types::{DataKey, WrapRecord};

    let user = <Address as TestAddress>::generate(&env);
    let period = symbol_short!("2024_01");
    let wrap_key = DataKey::Wrap(user.clone(), period.clone());

    let test_record = WrapRecord {
        timestamp: 1234567890,
        data_hash: BytesN::from_array(&env, &[99u8; 32]),
        archetype: symbol_short!("test"),
        period: period.clone(),
    };

    // Manually write to storage
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&wrap_key, &test_record);
    });

    // Query through contract
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let retrieved = client.get_wrap(&user, &period);

    // Verify exact match
    assert!(retrieved.is_some());
    let wrap = retrieved.unwrap();
    assert_eq!(wrap.timestamp, 1234567890);
    assert_eq!(wrap.data_hash, BytesN::from_array(&env, &[99u8; 32]));
    assert_eq!(wrap.archetype, symbol_short!("test"));
    assert_eq!(wrap.period, period);
}

#[test]
fn test_get_count_state_verification() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);

    // Manually inject count into storage
    use storage_types::DataKey;

    let user = <Address as TestAddress>::generate(&env);
    let count_key = DataKey::WrapCount(user.clone());

    // Manually write count to storage
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&count_key, &5u32);
    });

    // Query through contract
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let count = client.get_count(&user);

    // Verify exact match
    assert_eq!(count, 5);
}

#[test]
fn test_get_admin_state_verification() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);

    // Manually inject admin into storage
    use storage_types::DataKey;

    let admin = <Address as TestAddress>::generate(&env);
    let admin_key = DataKey::Admin;

    // Manually write admin to storage
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&admin_key, &admin);
    });

    // Query through contract
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let retrieved_admin = client.get_admin();

    // Verify exact match
    assert!(retrieved_admin.is_some());
    assert_eq!(retrieved_admin.unwrap(), admin);
}

// ============================================================================
// SEP-41 Token Interface Tests
// ============================================================================

#[test]
fn test_token_metadata() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Test decimals - must return 0
    assert_eq!(client.decimals(), 0);

    // Test name - must return "Stellar Wrap Registry"
    let name = client.name();
    assert_eq!(
        name,
        soroban_sdk::String::from_str(&env, "Stellar Wrap Registry")
    );

    // Test symbol - must return "WRAP"
    let symbol = client.symbol();
    assert_eq!(symbol, soroban_sdk::String::from_str(&env, "WRAP"));
}

#[test]
fn test_balance_of() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Create mock admin and user addresses
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Create a mock public key (32 bytes)
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract with admin and public key
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    // Initially, balance should be 0
    assert_eq!(client.balance_of(&user), 0);

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");

    // Mint first wrap
    let period_1 = symbol_short!("2024_01");
    client.mint_wrap(&user, &dummy_hash, &archetype, &period_1);
    assert_eq!(client.balance_of(&user), 1);

    // Mint second wrap
    let period_2 = symbol_short!("2024_02");
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    client.mint_wrap(&user, &dummy_hash_2, &archetype, &period_2);
    assert_eq!(client.balance_of(&user), 2);

    // Mint third wrap
    let period_3 = symbol_short!("2024_03");
    let dummy_hash_3 = BytesN::from_array(&env, &[123u8; 32]);
    client.mint_wrap(&user, &dummy_hash_3, &archetype, &period_3);
    assert_eq!(client.balance_of(&user), 3);

    // Test balance for different user (should be 0)
    let other_user = Address::generate(&env);
    assert_eq!(client.balance_of(&other_user), 0);
}

#[test]
fn test_allowance_always_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    // Allowance should always be 0 for Soulbound Tokens
    assert_eq!(client.allowance(&user1, &user2), 0);

    // Even after attempting to approve (which will panic), allowance should be checked before
    // Since we can't call approve successfully, we just verify the read function
    assert_eq!(client.allowance(&user1, &user2), 0);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_transfer_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let from = Address::generate(&env);
    let to = Address::generate(&env);

    // Attempting to transfer should panic immediately
    client.transfer(&from, &to, &1);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_transfer_from_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let spender = Address::generate(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    // Attempting to transfer_from should panic immediately
    client.transfer_from(&spender, &from, &to, &1);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_approve_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let from = Address::generate(&env);
    let spender = Address::generate(&env);

    // Attempting to approve should panic immediately
    // expiration_ledger can be any value since it won't be reached
    client.approve(&from, &spender, &1, &1000);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_burn_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    // Attempting to burn should panic immediately
    client.burn(&user, &1);
}

#[test]
fn test_balance_increments_on_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Create mock admin and user addresses
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Create a mock public key (32 bytes)
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);

    // Initialize contract with admin and public key
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let _dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban");

    // Verify initial state
    assert_eq!(client.balance_of(&user), 0);

    // Mint 5 wraps across different periods
    let periods = [
        symbol_short!("2024_01"),
        symbol_short!("2024_02"),
        symbol_short!("2024_03"),
        symbol_short!("2024_04"),
        symbol_short!("2024_05"),
    ];

    for (i, period) in periods.iter().enumerate() {
        let mut hash_data = [0u8; 32];
        hash_data[0] = (i + 1) as u8;
        let hash = BytesN::from_array(&env, &hash_data);
        client.mint_wrap(&user, &hash, &archetype, period);
        assert_eq!(client.balance_of(&user), (i + 1) as i128);
    }

    // Final balance should be 5
    assert_eq!(client.balance_of(&user), 5);
}
