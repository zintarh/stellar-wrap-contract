#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    xdr::{ContractId, ScAddress, ScVal},
    Address, BytesN, Env, Symbol, TryIntoVal, Val,
};

use crate::storage_types::{WrapLifecycleFSM, WrapRecord, WrapState};

// ─── FSM transition unit tests ──────────────────────────────────────────

#[test]
fn test_fsm_draft_to_expired() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Draft, 1000);
    assert!(fsm.can_transition_to(&WrapState::Expired));
    assert!(fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Expired);
    assert_eq!(fsm.updated_at, 2000);
}

#[test]
fn test_fsm_pending_to_expired() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Pending, 1000);
    assert!(fsm.can_transition_to(&WrapState::Expired));
    assert!(fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Expired);
    assert_eq!(fsm.updated_at, 2000);
}

#[test]
fn test_fsm_active_cannot_expire() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Active, 1000);
    assert!(!fsm.can_transition_to(&WrapState::Expired));
    assert!(!fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Active);
}

#[test]
fn test_fsm_archived_cannot_expire() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Archived, 1000);
    assert!(!fsm.can_transition_to(&WrapState::Expired));
    assert!(!fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Archived);
}

#[test]
fn test_fsm_cancelled_cannot_expire() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Cancelled, 1000);
    assert!(!fsm.can_transition_to(&WrapState::Expired));
    assert!(!fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Cancelled);
}

#[test]
fn test_fsm_expired_cannot_expire_again() {
    let mut fsm = WrapLifecycleFSM::new(WrapState::Expired, 1000);
    assert!(!fsm.can_transition_to(&WrapState::Expired));
    assert!(!fsm.transition_to(WrapState::Expired, 2000));
    assert_eq!(fsm.state, WrapState::Expired);
}

#[test]
fn test_fsm_expired_has_no_exit_transitions() {
    let fsm = WrapLifecycleFSM::new(WrapState::Expired, 1000);
    // Once expired, a wrap cannot transition to any other state.
    assert!(!fsm.can_transition_to(&WrapState::Draft));
    assert!(!fsm.can_transition_to(&WrapState::Pending));
    assert!(!fsm.can_transition_to(&WrapState::Active));
    assert!(!fsm.can_transition_to(&WrapState::Archived));
    assert!(!fsm.can_transition_to(&WrapState::Cancelled));
}

// ─── Helper: insert a wrap record in a given state ──────────────────────

fn insert_wrap_in_state(
    env: &Env,
    contract_id: &Address,
    user: &Address,
    period: u64,
    state: WrapState,
    updated_at: u64,
) {
    env.as_contract(contract_id, || {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let record = WrapRecord {
            timestamp: updated_at,
            data_hash: BytesN::from_array(env, &[9u8; 32]),
            archetype: symbol_short!("arch"),
            period,
            description: None,
            image_url: None,
            fsm: WrapLifecycleFSM::new(state, updated_at),
        };
        env.storage().persistent().set(&wrap_key, &record);
    });
}

// ─── expire_wrap integration tests ──────────────────────────────────────

#[test]
fn test_expire_draft_wrap_after_deadline_succeeds() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);
    let period = 202501u64;
    let insertion_time = 1000000u64;

    env.mock_all_auths();
    client.initialize(&admin, &pubkey);

    // Insert a Draft wrap directly with a known timestamp.
    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    // Advance the ledger beyond the 7-day default expiration window.
    let default_duration: u64 = 7 * 24 * 60 * 60; // 604800 seconds
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.fsm.state, WrapState::Expired);
    assert_eq!(wrap.fsm.updated_at, insertion_time + default_duration + 1);
}

#[test]
fn test_expire_pending_wrap_after_deadline_succeeds() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[2u8; 32]);
    let user = Address::generate(&env);
    let period = 202502u64;
    let insertion_time = 2000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Pending,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 100;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.fsm.state, WrapState::Expired);
}

#[test]
#[should_panic(expected = "Error(Contract, #46)")]
fn test_expire_wrap_before_deadline_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[3u8; 32]);
    let user = Address::generate(&env);
    let period = 202503u64;
    let insertion_time = 3000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    // Only advance 1 day (well within the 7-day default).
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + 86400;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_expire_nonexistent_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[4u8; 32]);
    let user = Address::generate(&env);
    let period = 202599u64;

    client.initialize(&admin, &pubkey);

    env.ledger().with_mut(|li| {
        li.timestamp = 9999999;
    });

    env.mock_all_auths();
    // No wrap was ever created — must panic with WrapNotFound.
    client.expire_wrap(&user, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_expire_active_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[5u8; 32]);
    let user = Address::generate(&env);
    let period = 202504u64;
    let insertion_time = 4000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Active,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_expire_archived_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[6u8; 32]);
    let user = Address::generate(&env);
    let period = 202505u64;
    let insertion_time = 5000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Archived,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_expire_cancelled_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[7u8; 32]);
    let user = Address::generate(&env);
    let period = 202506u64;
    let insertion_time = 6000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Cancelled,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_expire_already_expired_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[8u8; 32]);
    let user = Address::generate(&env);
    let period = 202507u64;
    let insertion_time = 7000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Expired,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

// ─── Expiration deadline edge cases ─────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #46)")]
fn test_expire_at_exact_deadline_boundary_fails() {
    // The check is `now < expires_at`, so at exactly the deadline the wrap
    // is NOT yet expired (strict less-than).
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[9u8; 32]);
    let user = Address::generate(&env);
    let period = 202508u64;
    let insertion_time = 8000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration; // exactly at boundary
    });

    env.mock_all_auths();
    // Should panic because now == expires_at is not "greater than"
    client.expire_wrap(&user, &period);
}

#[test]
fn test_expire_just_past_deadline_succeeds() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[10u8; 32]);
    let user = Address::generate(&env);
    let period = 202509u64;
    let insertion_time = 9000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Pending,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1; // one second past
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.fsm.state, WrapState::Expired);
}

// ─── Expiration event emission ──────────────────────────────────────────

#[test]
fn test_expire_wrap_emits_event() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[11u8; 32]);
    let user = Address::generate(&env);
    let period = 202510u64;
    let insertion_time = 1000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);

    let all_events = env.events().all();
    let last_event = all_events
        .events()
        .last()
        .expect("Expected at least one event");
    let sc_contract_id: ContractId = last_event
        .contract_id
        .as_ref()
        .expect("expected contract id")
        .clone();
    let contract_val: Val = ScVal::Address(ScAddress::Contract(sc_contract_id))
        .try_into_val(&env)
        .unwrap();
    let event_contract: Address = contract_val.try_into_val(&env).unwrap();
    assert_eq!(event_contract, contract_id);

    let (topics, data) = crate::test_utils::decode_events(&env)
        .pop()
        .expect("Expected at least one event");

    let topic_0: Symbol = topics[0].try_into_val(&env).unwrap();
    let topic_1: Address = topics[1].try_into_val(&env).unwrap();
    let topic_2: u64 = topics[2].try_into_val(&env).unwrap();

    assert_eq!(topic_0, symbol_short!("expire"));
    assert_eq!(topic_1, user);
    assert_eq!(topic_2, period);

    let event_data: Symbol = data.try_into_val(&env).unwrap();
    assert_eq!(event_data, symbol_short!("expired"));
}

// ─── set_expiration_duration / expiration_duration tests ────────────────

#[test]
fn test_default_expiration_duration_is_seven_days() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[12u8; 32]);

    client.initialize(&admin, &pubkey);

    let expected: u64 = 7 * 24 * 60 * 60;
    assert_eq!(client.expiration_duration(), expected);
}

#[test]
fn test_set_and_get_custom_expiration_duration() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[13u8; 32]);

    client.initialize(&admin, &pubkey);

    // Default is 7 days.
    let default: u64 = 7 * 24 * 60 * 60;
    assert_eq!(client.expiration_duration(), default);

    // Set to 30 days.
    let custom_duration: u64 = 30 * 24 * 60 * 60;
    env.mock_all_auths();
    client.set_expiration_duration(&custom_duration);
    assert_eq!(client.expiration_duration(), custom_duration);
}

#[test]
fn test_custom_duration_affects_expire_behavior() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[14u8; 32]);
    let user = Address::generate(&env);
    let period = 202511u64;
    let insertion_time = 11000000u64;

    client.initialize(&admin, &pubkey);

    // Set expiration to just 1 hour (3600 seconds).
    env.mock_all_auths();
    client.set_expiration_duration(&3600);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    // Advance 2 hours — well past the 1-hour custom duration.
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + 7200;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.fsm.state, WrapState::Expired);
}

#[test]
#[should_panic(expected = "Error(Contract, #47)")]
fn test_set_expiration_duration_zero_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[15u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();
    client.set_expiration_duration(&0);
}

#[test]
#[should_panic]
fn test_set_expiration_duration_non_admin_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[16u8; 32]);

    client.initialize(&admin, &pubkey);

    // Do NOT mock auths — require_auth will panic for non-admin.
    client.set_expiration_duration(&3600);
}

// ─── Pause interaction ──────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_expire_wrap_when_paused_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[17u8; 32]);
    let user = Address::generate(&env);
    let period = 202512u64;
    let insertion_time = 12000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    // Pause the contract, then try to expire.
    env.mock_all_auths();
    client.pause();
    client.expire_wrap(&user, &period);
}

// ─── Saturation edge case ───────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #46)")]
fn test_expire_with_max_timestamp_does_not_overflow() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[18u8; 32]);
    let user = Address::generate(&env);
    let period = 202513u64;
    let insertion_time = u64::MAX;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    // The deadline saturates at u64::MAX, and the current time is way below
    // that, so the wrap is not yet expired.
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period);
}

// ─── Permissionless design verification ─────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #46)")]
fn test_expire_wrap_does_not_require_auth() {
    // expire_wrap is designed to be callable by anyone — it should work
    // even without any mocked authorizations. Here we call it before the
    // deadline, so it panics with WrapNotExpired, confirming that auth
    // did NOT fail first (which would be a host-level panic).
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[22u8; 32]);
    let user = Address::generate(&env);
    let period = 202704u64;
    let insertion_time = 15000000u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period,
        WrapState::Draft,
        insertion_time,
    );

    // Do NOT mock any auths — the call should reach the deadline check,
    // not fail on auth.
    client.expire_wrap(&user, &period);
}

// ─── Multiple wraps for same user ───────────────────────────────────────

#[test]
fn test_expire_one_wrap_does_not_affect_others() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[19u8; 32]);
    let user = Address::generate(&env);
    let period_a = 202601u64;
    let period_b = 202602u64;
    let insertion_time = 13000000u64;

    client.initialize(&admin, &pubkey);

    // Insert two draft wraps.
    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period_a,
        WrapState::Draft,
        insertion_time,
    );
    insert_wrap_in_state(
        &env,
        &contract_id,
        &user,
        period_b,
        WrapState::Active,
        insertion_time,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user, &period_a);

    // period_a should be expired.
    let wrap_a = client.get_wrap(&user, &period_a).unwrap();
    assert_eq!(wrap_a.fsm.state, WrapState::Expired);

    // period_b should remain Active.
    let wrap_b = client.get_wrap(&user, &period_b).unwrap();
    assert_eq!(wrap_b.fsm.state, WrapState::Active);
}

// ─── Multiple users ─────────────────────────────────────────────────────

#[test]
fn test_expire_multiple_users_independently() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[20u8; 32]);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let period = 202701u64;
    let insertion_time_a = 14000000u64;
    let insertion_time_b = 14000100u64;

    client.initialize(&admin, &pubkey);

    insert_wrap_in_state(
        &env,
        &contract_id,
        &user_a,
        period,
        WrapState::Draft,
        insertion_time_a,
    );
    insert_wrap_in_state(
        &env,
        &contract_id,
        &user_b,
        period,
        WrapState::Draft,
        insertion_time_b,
    );

    let default_duration: u64 = 7 * 24 * 60 * 60;
    env.ledger().with_mut(|li| {
        li.timestamp = insertion_time_a + default_duration + 1;
    });

    env.mock_all_auths();
    client.expire_wrap(&user_a, &period);

    // user_a should be expired.
    let wrap_a = client.get_wrap(&user_a, &period).unwrap();
    assert_eq!(wrap_a.fsm.state, WrapState::Expired);

    // user_b should still be Draft (its own deadline is later).
    let wrap_b = client.get_wrap(&user_b, &period).unwrap();
    assert_eq!(wrap_b.fsm.state, WrapState::Draft);
}
