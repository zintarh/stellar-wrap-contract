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
    
    // Mint wrap as admin for the user
    client.mint_wrap(&user, &dummy_hash, &archetype);
    
    // Retrieve the wrap record
    let wrap_opt = client.get_wrap(&user);
    
    // Assert the wrap exists and matches what was minted
    assert!(wrap_opt.is_some());
    let wrap = wrap_opt.unwrap();
    
    assert_eq!(wrap.data_hash, dummy_hash);
    assert_eq!(wrap.archetype, archetype);
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
    
    // This should fail because unauthorized is not the admin
    client.mint_wrap(&user, &dummy_hash, &archetype);
}
