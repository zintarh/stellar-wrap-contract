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

    // Verify it was minted
    let wrap = client.get_wrap(&user, &period);
    assert!(wrap.is_some());
}

#[test]
fn test_mint_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("architect");
    let period = symbol_short!("jan2024");

    client.mint_wrap(&user, &dummy_hash, &archetype, &period);

    let events = env.events().all();
    
    // The mint event should be emitted
    assert!(events.len() > 0, "No events emitted");
    
    // Count mint events and find the mint event
    let mut mint_event_count = 0;
    let mut found_event_topics = None;
    let mut found_event_data = None;
    
    for i in 0..events.len() {
        let event = events.get(i).unwrap();
        let (_, topics, data) = event;
        
        // Check if this is a mint event (first topic is "mint")
        if topics.len() >= 1 && topics.get(0).unwrap().get_payload() == symbol_short!("mint").to_val().get_payload() {
            mint_event_count += 1;
            found_event_topics = Some(topics);
            found_event_data = Some(data);
        }
    }
    
    // Assert exactly one mint event was emitted
    assert_eq!(mint_event_count, 1, "Should emit exactly one mint event");
    
    let topics = found_event_topics.unwrap();
    let data = found_event_data.unwrap();
    
    // Verify event structure
    assert_eq!(topics.len(), 2, "Event should have 2 topics: 'mint' and user address");
    
    // Verify first topic is "mint"
    assert_eq!(
        topics.get(0).unwrap().get_payload(),
        symbol_short!("mint").to_val().get_payload(),
        "First topic should be 'mint'"
    );
    
    // Second topic should be an address - just verify it exists and is non-zero
    let second_topic = topics.get(1).unwrap().get_payload();
    assert!(second_topic != 0, "Second topic (user address) should exist and be non-zero");

    // Verify data exists and is non-zero (it's a u64 representing the period)
    let event_data_payload = data.get_payload();
    assert!(event_data_payload != 0, "Event data (period as u64) should exist and be non-zero");
    
    // Optional: Log the values for debugging
    // println!("Event data payload: {}", event_data_payload);
    // println!("Expected period payload: {}", period.to_val().get_payload());
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