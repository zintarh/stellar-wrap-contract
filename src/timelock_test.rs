#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, Symbol, TryIntoVal, Val,
};

use crate::storage_types::TimelockAction;
use crate::timelock::GRACE_PERIOD;

#[test]
fn test_timelock_execution_just_inside_grace_window() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let initial_time = 1000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = initial_time;
    });

    let delay_seconds = 3600u64;
    env.mock_all_auths();
    client.enable_timelock(&delay_seconds);

    let new_pubkey = BytesN::from_array(&env, &[99u8; 32]);
    let action = TimelockAction::SetAdminPubKey(new_pubkey.clone());

    env.mock_all_auths();
    let id = client.timelock_schedule(&action);

    // eta = 1000 + 3600 = 4600
    // grace_deadline = 4600 + GRACE_PERIOD = 4600 + 1_209_600 = 1_214_200
    let eta = initial_time + delay_seconds;
    let grace_deadline = eta + GRACE_PERIOD;

    env.ledger().with_mut(|li| {
        li.timestamp = grace_deadline;
    });

    env.mock_all_auths();
    client.timelock_execute(&id);

    // Verify action took effect
    assert_eq!(client.get_admin_pubkey().unwrap(), new_pubkey);
    assert_eq!(client.timelock_pending().len(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #66)")]
fn test_timelock_execution_just_outside_grace_window_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let initial_time = 1000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = initial_time;
    });

    let delay_seconds = 3600u64;
    env.mock_all_auths();
    client.enable_timelock(&delay_seconds);

    let new_pubkey = BytesN::from_array(&env, &[88u8; 32]);
    let action = TimelockAction::SetAdminPubKey(new_pubkey);

    env.mock_all_auths();
    let id = client.timelock_schedule(&action);

    let eta = initial_time + delay_seconds;
    let past_grace_deadline = eta + GRACE_PERIOD + 1;

    env.ledger().with_mut(|li| {
        li.timestamp = past_grace_deadline;
    });

    env.mock_all_auths();
    // Attempting execution 1 second past grace period must fail with TimelockOperationExpired (#66)
    client.timelock_execute(&id);
}

#[test]
fn test_timelock_sweep_expired_operation_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let initial_time = 1000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = initial_time;
    });

    let delay_seconds = 3600u64;
    env.mock_all_auths();
    client.enable_timelock(&delay_seconds);

    let action = TimelockAction::SetAdminPubKey(BytesN::from_array(&env, &[77u8; 32]));

    env.mock_all_auths();
    let id = client.timelock_schedule(&action);

    assert_eq!(client.timelock_pending().len(), 1);
    assert!(client.timelock_operation(&id).is_some());

    let eta = initial_time + delay_seconds;
    let past_grace_deadline = eta + GRACE_PERIOD + 1;

    env.ledger().with_mut(|li| {
        li.timestamp = past_grace_deadline;
    });

    // Sweep call requires no auth (permissionless)
    client.timelock_sweep_expired(&id);

    // Verify operation is removed from storage and pending list
    assert_eq!(client.timelock_pending().len(), 0);
    assert!(client.timelock_operation(&id).is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #67)")]
fn test_timelock_sweep_unexpired_operation_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let initial_time = 1000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = initial_time;
    });

    let delay_seconds = 3600u64;
    env.mock_all_auths();
    client.enable_timelock(&delay_seconds);

    let action = TimelockAction::SetAdminPubKey(BytesN::from_array(&env, &[66u8; 32]));

    env.mock_all_auths();
    let id = client.timelock_schedule(&action);

    // Advance to eta + 1 (ready for execution, but unexpired)
    let eta = initial_time + delay_seconds;
    env.ledger().with_mut(|li| {
        li.timestamp = eta + 1;
    });

    // Sweeping unexpired operation must panic with TimelockOperationNotExpired (#67)
    client.timelock_sweep_expired(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_timelock_sweep_nonexistent_operation_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let fake_id = BytesN::from_array(&env, &[9u8; 32]);
    client.timelock_sweep_expired(&fake_id);
}

#[test]
fn test_timelock_sweep_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let initial_time = 1000u64;
    env.ledger().with_mut(|li| {
        li.timestamp = initial_time;
    });

    let delay_seconds = 3600u64;
    env.mock_all_auths();
    client.enable_timelock(&delay_seconds);

    let action = TimelockAction::SetAdminPubKey(BytesN::from_array(&env, &[55u8; 32]));

    env.mock_all_auths();
    let id = client.timelock_schedule(&action);

    let eta = initial_time + delay_seconds;
    env.ledger().with_mut(|li| {
        li.timestamp = eta + GRACE_PERIOD + 100;
    });

    client.timelock_sweep_expired(&id);

    let events = crate::test_utils::decode_events(&env);
    let (topics, data) = events.last().expect("Expected sweep event");

    let topic_0: Symbol = topics[0].try_into_val(&env).unwrap();
    let topic_1: Symbol = topics[1].try_into_val(&env).unwrap();
    let event_data: BytesN<32> = data.try_into_val(&env).unwrap();

    assert_eq!(topic_0, symbol_short!("timelock"));
    assert_eq!(topic_1, symbol_short!("sweep"));
    assert_eq!(event_data, id);
}
