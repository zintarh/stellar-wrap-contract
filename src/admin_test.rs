#![cfg(test)]

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::test_utils::decode_events;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, TryIntoVal};

/// Registers the contract and initializes it with a freshly generated admin.
fn setup(env: &Env) -> (StellarWrapContractClient<'_>, Address) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let admin_pubkey = BytesN::from_array(env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    (client, admin)
}

#[test]
fn test_redundant_pause_does_not_emit_an_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Genuine initial pause transition emits exactly one event.
    client.pause();
    let genuine = decode_events(&env);
    assert_eq!(genuine.len(), 1, "a genuine pause must emit exactly one event");

    // A redundant pause (already paused) must emit no additional event.
    assert!(client.is_paused());
    client.pause();
    let redundant = decode_events(&env);
    assert_eq!(redundant.len(), 0, "a redundant pause must not emit an event");
}

#[test]
fn test_redundant_unpause_does_not_emit_an_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.pause();
    decode_events(&env); // drain the pause event

    // Genuine unpause transition emits exactly one event.
    client.unpause();
    let genuine = decode_events(&env);
    assert_eq!(genuine.len(), 1, "a genuine unpause must emit exactly one event");

    // A redundant unpause (already unpaused) must emit no additional event.
    assert!(!client.is_paused());
    client.unpause();
    let redundant = decode_events(&env);
    assert_eq!(redundant.len(), 0, "a redundant unpause must not emit an event");
}

#[test]
fn test_pause_and_unpause_are_distinguishable_by_topic_and_carry_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    // Pause: topic is ("pause", "paused"), payload is the acting admin.
    client.pause();
    let pause_events = decode_events(&env);
    assert_eq!(pause_events.len(), 1, "expected a single pause event");
    let (pause_topics, pause_data) = pause_events.last().expect("no pause event");
    let pause_action: Symbol = pause_topics[0].try_into_val(&env).unwrap();
    let pause_direction: Symbol = pause_topics[1].try_into_val(&env).unwrap();
    let pause_admin: Address = pause_data.clone().try_into_val(&env).unwrap();
    assert_eq!(pause_action, symbol_short!("pause"));
    assert_eq!(pause_direction, symbol_short!("paused"));
    assert_eq!(pause_admin, admin);

    // Unpause: topic is ("pause", "unpaused"), payload is the acting admin.
    client.unpause();
    let unpause_events = decode_events(&env);
    assert_eq!(unpause_events.len(), 1, "expected a single unpause event");
    let (unpause_topics, unpause_data) = unpause_events.last().expect("no unpause event");
    let unpause_action: Symbol = unpause_topics[0].try_into_val(&env).unwrap();
    let unpause_direction: Symbol = unpause_topics[1].try_into_val(&env).unwrap();
    let unpause_admin: Address = unpause_data.clone().try_into_val(&env).unwrap();
    assert_eq!(unpause_action, symbol_short!("pause"));
    assert_eq!(unpause_direction, symbol_short!("unpaused"));
    assert_eq!(unpause_admin, admin);
}

#[test]
fn test_schema_version_initialized_at_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Schema version should be 1 after initialization
    let schema_version = client.schema_version();
    assert_eq!(schema_version, 1, "schema version should be 1 after initialization");
}