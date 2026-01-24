#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as TestAddress, Address, BytesN, Env};

#[test]
fn test_minting_flow() {
    let env = Env::default();

    // Register the contract
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Create mock admin and user addresses
    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    // Initialize contract with admin
    client.initialize(&admin);

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
#[should_panic]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);

    // First initialization should succeed
    client.initialize(&admin);

    // Second initialization should fail
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);
    let unauthorized = <Address as TestAddress>::generate(&env);

    // Initialize contract
    client.initialize(&admin);

    // Try to mint as unauthorized user (without mock_all_auths, this will fail)
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi");
    let period = symbol_short!("2024_01");

    // This should fail because unauthorized is not the admin
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);
}

#[test]
fn test_multiple_periods() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    // Initialize contract
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype_1 = symbol_short!("soroban");
    let archetype_2 = symbol_short!("defi");
    let period_1 = symbol_short!("2024_01"); // January
    let period_2 = symbol_short!("2024_02"); // February

    // Mint wrap for January
    client.mint_wrap(&user, &dummy_hash_1, &archetype_1, &period_1);

    // Mint wrap for February (should succeed - different period)
    client.mint_wrap(&user, &dummy_hash_2, &archetype_2, &period_2);

    // Retrieve both wraps
    let wrap_jan = client.get_wrap(&user, &period_1).unwrap();
    let wrap_feb = client.get_wrap(&user, &period_2).unwrap();

    // Assert they are different
    assert_eq!(wrap_jan.data_hash, dummy_hash_1);
    assert_eq!(wrap_jan.archetype, archetype_1);
    assert_eq!(wrap_jan.period, period_1);

    assert_eq!(wrap_feb.data_hash, dummy_hash_2);
    assert_eq!(wrap_feb.archetype, archetype_2);
    assert_eq!(wrap_feb.period, period_2);
}

#[test]
#[should_panic]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    // Initialize contract
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("soroban");
    let period = symbol_short!("2024_01");

    // Mint first wrap
    client.mint_wrap(&user, &dummy_hash_1, &archetype, &period);

    // Try to mint again for the same period (should fail)
    client.mint_wrap(&user, &dummy_hash_2, &archetype, &period);
}

#[test]
fn test_update_admin_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    
    let admin = TestAddress::generate(&env);
    let new_admin = TestAddress::generate(&env);
    
    // Initialize contract with admin
    client.initialize(&admin);
    
    // Set up authorization for admin
    env.mock_all_auths();
    
    // Update admin (should succeed)
    client.update_admin(&new_admin);
    
    // Verify new admin can mint (proving the update worked)
    let user = TestAddress::generate(&env);
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban_architect");
    let period = symbol_short!("2024-01");
    
    // This should succeed because new_admin is now the admin
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);
    
    // Verify the wrap was created
    let wrap_opt = client.get_wrap(&user, &period);
    assert!(wrap_opt.is_some());
}

#[test]
#[should_panic(expected = "Error(ContractError(3))")]
fn test_update_admin_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    
    let admin = TestAddress::generate(&env);
    let unauthorized = TestAddress::generate(&env);
    let new_admin = TestAddress::generate(&env);
    
    // Initialize contract
    client.initialize(&admin);
    
    // Don't set up mock_all_auths - this means require_auth will fail
    // Try to update admin as unauthorized user (should fail with Unauthorized error)
    client.update_admin(&new_admin);
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

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    // Initialize contract
    client.initialize(&admin);
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
    let other_user = <Address as TestAddress>::generate(&env);
    assert_eq!(client.balance_of(&other_user), 0);
}

#[test]
fn test_allowance_always_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user1 = <Address as TestAddress>::generate(&env);
    let user2 = <Address as TestAddress>::generate(&env);

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

    let from = <Address as TestAddress>::generate(&env);
    let to = <Address as TestAddress>::generate(&env);

    // Attempting to transfer should panic immediately
    client.transfer(&from, &to, &1);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_transfer_from_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let spender = <Address as TestAddress>::generate(&env);
    let from = <Address as TestAddress>::generate(&env);
    let to = <Address as TestAddress>::generate(&env);

    // Attempting to transfer_from should panic immediately
    client.transfer_from(&spender, &from, &to, &1);
}

#[test]
#[should_panic(expected = "SBT: Transfer not allowed")]
fn test_approve_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let from = <Address as TestAddress>::generate(&env);
    let spender = <Address as TestAddress>::generate(&env);

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

    let user = <Address as TestAddress>::generate(&env);

    // Attempting to burn should panic immediately
    client.burn(&user, &1);
}

#[test]
fn test_balance_increments_on_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = <Address as TestAddress>::generate(&env);
    let user = <Address as TestAddress>::generate(&env);

    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
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
