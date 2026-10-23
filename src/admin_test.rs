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
    assert_eq!(
        genuine.len(),
        1,
        "a genuine pause must emit exactly one event"
    );

    // A redundant pause (already paused) must emit no additional event.
    assert!(client.is_paused());
    client.pause();
    let redundant = decode_events(&env);
    assert_eq!(
        redundant.len(),
        0,
        "a redundant pause must not emit an event"
    );
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
    assert_eq!(
        genuine.len(),
        1,
        "a genuine unpause must emit exactly one event"
    );

    // A redundant unpause (already unpaused) must emit no additional event.
    assert!(!client.is_paused());
    client.unpause();
    let redundant = decode_events(&env);
    assert_eq!(
        redundant.len(),
        0,
        "a redundant unpause must not emit an event"
    );
}

#[test]
fn test_pause_and_unpause_are_distinguishable_by_topic_and_carry_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Both publish `v1 / admin / pause`; the payload says which direction.
    let assert_pause_event = |paused: bool| {
        let events = decode_events(&env);
        assert_eq!(events.len(), 1, "expected a single pause event");
        let (topics, data) = events.last().expect("no pause event");
        let names: std::vec::Vec<Symbol> =
            topics.iter().map(|t| t.try_into_val(&env).unwrap()).collect();
        assert_eq!(
            names,
            std::vec![symbol_short!("v1"), symbol_short!("admin"), symbol_short!("pause")]
        );
        let event: crate::events::Event = data.clone().try_into_val(&env).unwrap();
        assert_eq!(event, crate::events::Event::AdminPause(paused));
    };

    client.pause();
    assert_pause_event(true);

    client.unpause();
    assert_pause_event(false);
}

#[test]
fn test_schema_version_initialized_at_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Schema version should be 1 after initialization
    let schema_version = client.schema_version();
    assert_eq!(
        schema_version, 1,
        "schema version should be 1 after initialization"
    );
}
