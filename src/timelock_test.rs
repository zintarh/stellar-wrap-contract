#![cfg(test)]

extern crate std;

use super::*;
use crate::storage_types::{FeeParams, StakeConfig, TimelockAction};
use crate::timelock;
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    xdr, Address, BytesN, Env, Symbol,
};

#[test]
fn test_timelock_flow() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    // 1. enable_timelock(MIN_DELAY) succeeds and timelock_delay() returns it.
    client.enable_timelock(&timelock::MIN_DELAY);
    assert_eq!(client.timelock_delay(), Some(timelock::MIN_DELAY));

    // 2. timelock_schedule(SetAdmin(new)) returns an id matching timelock_operation_id(action).
    let new_admin = Address::generate(&env);
    let action = TimelockAction::SetAdmin(new_admin.clone());
    let expected_id = client.timelock_operation_id(&action);

    let now = 12345;
    env.ledger().set_timestamp(now);

    let id = client.timelock_schedule(&action);
    assert_eq!(id, expected_id);

    // The sched event is emitted by the scheduling invocation.
    assert!(
        has_timelock_event(&env, symbol_short!("sched")),
        "sched event not found"
    );

    // 3. timelock_operation(id) returns the queued operation with the expected eta and scheduled_at.
    let op = client.timelock_operation(&id).unwrap();
    assert_eq!(op.scheduled_at, now);
    assert_eq!(op.eta, now + timelock::MIN_DELAY);

    // 4. timelock_pending() contains the id.
    let pending = client.timelock_pending();
    assert!(pending.contains(&id));

    // 5. After advancing past eta, timelock_execute(id) applies the action and removes it from pending.
    env.ledger().set_timestamp(now + timelock::MIN_DELAY + 1);
    client.timelock_execute(&id);

    // The exec event is emitted by the executing invocation. (env.events().all()
    // only reports the last invocation, so assert before any further calls.)
    assert!(
        has_timelock_event(&env, symbol_short!("exec")),
        "exec event not found"
    );

    let pending_after = client.timelock_pending();
    assert!(!pending_after.contains(&id));

    // Check if new admin is applied
    assert_eq!(client.get_admin().unwrap(), new_admin);
}

/// True if the last invocation emitted a `[timelock, <action>]` event.
fn has_timelock_event(env: &Env, action: Symbol) -> bool {
    let timelock_scv = xdr::ScVal::from(symbol_short!("timelock"));
    let action_scv = xdr::ScVal::from(action);
    for event in env.events().all().events() {
        let body = match &event.body {
            xdr::ContractEventBody::V0(body) => body,
        };
        if body.topics.len() > 1 && body.topics[0] == timelock_scv && body.topics[1] == action_scv {
            return true;
        }
    }
    false
}

fn setup_contract(env: &Env) -> (StellarWrapContractClient<'static>, Address, BytesN<32>) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    (client, admin, admin_pubkey)
}

#[test]
fn test_timelock_lockout_direct_paths() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    // Enable timelock with a delay of 1 hour (3600 seconds)
    client.enable_timelock(&3600);

    // Test update_admin fails
    let new_admin = Address::generate(&env);
    let res = client.try_update_admin(&new_admin);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockRequired
    );

    // Test propose_admin fails
    let res = client.try_propose_admin(&new_admin);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockRequired
    );

    // Test accept_admin fails
    let res = client.try_accept_admin();
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockRequired
    );

    // Test upgrade fails
    let dummy_wasm = BytesN::from_array(&env, &[9u8; 32]);
    let res = client.try_upgrade(&dummy_wasm);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockRequired
    );
}

#[test]
fn test_timelock_scheduled_paths_succeed() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&3600);

    // Fast forward slightly to start clean
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    // Schedule SetAdmin
    let new_admin = Address::generate(&env);
    let action = TimelockAction::SetAdmin(new_admin.clone());
    let op_id = client.timelock_schedule(&action);

    // NOTE: no Upgrade action is scheduled here — executing an upgrade against
    // an arbitrary wasm hash is rejected by the soroban 27 host
    // (`Error(Storage, MissingValue)`) since no such blob is deployed.

    // Fast forward beyond the delay
    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + 3600 + 1;
    });

    // Execute all
    client.timelock_execute(&op_id);

    // Verification
    assert_eq!(client.get_admin(), Some(new_admin));
}

#[test]
fn test_open_paths_succeed_with_timelock() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&3600);

    // set_pause
    client.pause();
    assert_eq!(client.is_paused(), true);
    client.unpause();

    // set_transfer_fee
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.set_transfer_fee(&token, &recipient, &100);
    let fee = client.get_transfer_fee().unwrap();
    assert_eq!(fee.amount, 100);

    // set_fee_params
    let fee_params = FeeParams {
        base_fee: 100,
        per_kib_fee: 10,
        scale_step_kib: 1024,
        max_fee: 1000,
    };
    client.set_fee_params(&fee_params);
    let out_fee = client.fee_params();
    assert_eq!(out_fee.base_fee, 100);

    // set_stake_config
    let stake_config = StakeConfig {
        min_stake: 1000,
        priority_multiplier_bps: 1000,
        max_priority_bps: 5000,
        cooldown_seconds: 3600,
    };
    client.set_stake_config(&stake_config);
    let out_config = client.get_stake_config();
    assert_eq!(out_config.min_stake, 1000);

    // set_expiration_duration
    client.set_expiration_duration(&86400);
    assert_eq!(client.expiration_duration(), 86400);

    // set_bridge_relayer
    // Stores the single legacy relayer; the current contract API does not
    // expose a matching getter (only get_bridge_relayers(chain_id)), so we
    // only verify the call itself succeeds.
    let relayer = Address::generate(&env);
    client.set_bridge_relayer(&relayer);

    // Note: set_whitelist_root should technically fail because it has the guard,
    // but the issue requested documenting its behavior. We'll document the ones above which are truly open,
    // and below we'll test set_whitelist_root explicitly failing to show it is indeed guarded.
    let dummy_root = BytesN::from_array(&env, &[7u8; 32]);
    let res = client.try_set_whitelist_root(&dummy_root);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockRequired
    );
}

#[test]
fn test_direct_paths_succeed_without_timelock() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    // No timelock enabled yet

    let dummy_root = BytesN::from_array(&env, &[7u8; 32]);
    client.set_whitelist_root(&dummy_root);
    assert_eq!(client.get_whitelist_root(), Some(dummy_root));

    // NOTE: `upgrade` with an arbitrary wasm hash is deliberately not exercised
    // here — the soroban 27 host rejects a non-existent wasm with
    // `Error(Storage, MissingValue)`, so a "successful" upgrade would need a
    // real (deployed) wasm blob which this unit-test env does not provide.

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    env.mock_all_auths();
    client.accept_admin();
    assert_eq!(client.get_admin(), Some(new_admin.clone()));

    // update_admin skips the two step process
    let new_admin_2 = Address::generate(&env);
    client.update_admin(&new_admin_2);
    assert_eq!(client.get_admin(), Some(new_admin_2));
}

// ---------------------------------------------------------------------
// Negative & boundary tests for the timelock timing and duplicate-schedule
// guards (#689).
// ---------------------------------------------------------------------

/// Acceptance: executing one second before `eta` fails with
/// `TimelockNotReady` (#40).
#[test]
fn test_execute_one_second_before_eta_fails() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&timelock::MIN_DELAY);

    let now = 5_000;
    env.ledger().set_timestamp(now);

    let new_admin = Address::generate(&env);
    let id = client.timelock_schedule(&TimelockAction::SetAdmin(new_admin));

    // ETA = now + MIN_DELAY; one second short of it is still locked.
    env.ledger().set_timestamp(now + timelock::MIN_DELAY - 1);
    let res = client.try_timelock_execute(&id);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockNotReady
    );
}

/// Acceptance: executing at exactly `eta` succeeds — the `now < eta` check
/// makes the boundary inclusive.
#[test]
fn test_execute_at_exact_eta_succeeds() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&timelock::MIN_DELAY);

    let now = 5_000;
    env.ledger().set_timestamp(now);

    let new_admin = Address::generate(&env);
    let id = client.timelock_schedule(&TimelockAction::SetAdmin(new_admin.clone()));

    env.ledger().set_timestamp(now + timelock::MIN_DELAY);
    client.timelock_execute(&id);

    assert_eq!(client.get_admin(), Some(new_admin));
    assert!(client.timelock_pending().is_empty());
}

/// Acceptance: scheduling the same action twice fails with
/// `TimelockOperationExists` (#42). The operation id is derived from the
/// action payload alone (not the ETA), so a second queue of the same action
/// collides even after time advances.
#[test]
fn test_schedule_same_action_twice_fails() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&timelock::MIN_DELAY);

    let new_admin = Address::generate(&env);
    let action = TimelockAction::SetAdmin(new_admin);
    client.timelock_schedule(&action);

    let res = client.try_timelock_schedule(&action);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockOperationExists
    );
}

/// Acceptance: scheduling before `enable_timelock` fails with
/// `InvalidTimelockDelay` (#43) — the delay lookup comes back empty while the
/// timelock is still disabled.
#[test]
fn test_schedule_before_enable_fails() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    let new_admin = Address::generate(&env);
    let res = client.try_timelock_schedule(&TimelockAction::SetAdmin(new_admin));
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::InvalidTimelockDelay
    );
}

/// Acceptance: `MIN_DELAY - 1` is rejected, `MIN_DELAY` is accepted.
#[test]
fn test_enable_timelock_min_delay_boundary() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    let res = client.try_enable_timelock(&(timelock::MIN_DELAY - 1));
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::InvalidTimelockDelay
    );

    // Inclusive lower bound succeeds.
    client.enable_timelock(&timelock::MIN_DELAY);
    assert_eq!(client.timelock_delay(), Some(timelock::MIN_DELAY));
}

/// Acceptance: `MAX_DELAY + 1` is rejected, `MAX_DELAY` is accepted.
#[test]
fn test_enable_timelock_max_delay_boundary() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    let res = client.try_enable_timelock(&(timelock::MAX_DELAY + 1));
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::InvalidTimelockDelay
    );

    // Inclusive upper bound succeeds.
    client.enable_timelock(&timelock::MAX_DELAY);
    assert_eq!(client.timelock_delay(), Some(timelock::MAX_DELAY));
}

/// Acceptance: a second `enable_timelock` fails with
/// `TimelockAlreadyEnabled` (#45).
#[test]
fn test_enable_timelock_twice_fails() {
    let env = Env::default();
    let (client, _admin, _) = setup_contract(&env);

    client.enable_timelock(&timelock::MIN_DELAY);

    let res = client.try_enable_timelock(&timelock::MIN_DELAY);
    assert_eq!(
        ContractError::try_from(res.unwrap_err().unwrap()).unwrap(),
        ContractError::TimelockAlreadyEnabled
    );
}
