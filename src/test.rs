#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env,
};

#[test]
fn test_minting_flow() {
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

    let wrap_opt = client.get_wrap(&user, &period);

    assert!(wrap_opt.is_some());
    let wrap = wrap_opt.unwrap();

    assert_eq!(wrap.data_hash, dummy_hash);
    assert_eq!(wrap.archetype, archetype);
    assert_eq!(wrap.period, period);
    assert_eq!(wrap.timestamp, env.ledger().timestamp());
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);

    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi");
    let period = symbol_short!("jan2024");

    // Don't mock auth - this should fail
    client.mint_wrap(&user, &dummy_hash, &archetype, &period);
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

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype_1 = symbol_short!("architect");
    let archetype_2 = symbol_short!("patron");
    let period_1 = symbol_short!("jan2024");
    let period_2 = symbol_short!("feb2024");

    client.mint_wrap(&user, &dummy_hash_1, &archetype_1, &period_1);
    client.mint_wrap(&user, &dummy_hash_2, &archetype_2, &period_2);

    let wrap_jan = client.get_wrap(&user, &period_1).unwrap();
    let wrap_feb = client.get_wrap(&user, &period_2).unwrap();

    assert_eq!(wrap_jan.data_hash, dummy_hash_1);
    assert_eq!(wrap_jan.archetype, archetype_1);
    assert_eq!(wrap_jan.period, period_1);

    assert_eq!(wrap_feb.data_hash, dummy_hash_2);
    assert_eq!(wrap_feb.archetype, archetype_2);
    assert_eq!(wrap_feb.period, period_2);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("architect");
    let period = symbol_short!("jan2024");

    client.mint_wrap(&user, &dummy_hash_1, &archetype, &period);
    client.mint_wrap(&user, &dummy_hash_2, &archetype, &period);
}