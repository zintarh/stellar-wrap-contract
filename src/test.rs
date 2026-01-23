#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as TestAddress, Ledger},
    Address, BytesN, Env, Symbol,
};

#[test]
fn test_minting_flow() {
    let env = Env::default();

    // Register the contract
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Create mock admin and user addresses
    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);

    // Initialize contract with admin
    client.initialize(&admin);

    // Set up authorization for admin
    env.mock_all_auths();

    // Prepare dummy data for minting
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban_architect");
    let period = symbol_short!("2024-01"); // January 2024

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
#[should_panic(expected = "Error(ContractError(1))")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);

    // First initialization should succeed
    client.initialize(&admin);

    // Second initialization should fail
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Error(ContractError(3))")]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);
    let unauthorized = TestAddress::generate(&env);

    // Initialize contract
    client.initialize(&admin);

    // Try to mint as unauthorized user (without mock_all_auths, this will fail)
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi_degenerate");
    let period = symbol_short!("2024-01");

    // This should fail because unauthorized is not the admin
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);
}

#[test]
fn test_multiple_periods() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);

    // Initialize contract
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype_1 = symbol_short!("soroban_architect");
    let archetype_2 = symbol_short!("defi_patron");
    let period_1 = symbol_short!("2024-01"); // January
    let period_2 = symbol_short!("2024-02"); // February

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
#[should_panic(expected = "Error(ContractError(4))")]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);

    // Initialize contract
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("soroban_architect");
    let period = symbol_short!("2024-01");

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
}
