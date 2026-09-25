#![cfg(test)]

extern crate std;

use super::*;
use crate::storage_types::{FeeParams, StakeConfig, TimelockAction};
use crate::timelock;
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, IntoVal,
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

    let pending_after = client.timelock_pending();
    assert!(!pending_after.contains(&id));

    // Check if new admin is applied
    assert_eq!(client.get_admin().unwrap(), new_admin);

    // 6. sched and exec events are emitted.
    let events = env.events().all();
    let mut sched_found = false;
    let mut exec_found = false;
    for (_contract_id, topics, _data) in events.into_iter() {
        if topics.len() > 0 {
            if topics.get(0).unwrap().into_val(&env) == symbol_short!("timelock").into_val(&env) {
                if topics.len() > 1 {
                    let event_type = topics.get(1).unwrap().into_val(&env);
                    if event_type == symbol_short!("sched").into_val(&env) {
                        sched_found = true;
                    }
                    if event_type == symbol_short!("exec").into_val(&env) {
                        exec_found = true;
                    }
                }
            }
        }
    }
    assert!(sched_found, "sched event not found");
    assert!(exec_found, "exec event not found");
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
        res.unwrap_err().unwrap().unwrap().name(),
        "TimelockRequired"
    );

    // Test propose_admin fails
    let res = client.try_propose_admin(&new_admin);
    assert_eq!(
        res.unwrap_err().unwrap().unwrap().name(),
        "TimelockRequired"
    );

    // Test accept_admin fails
    let res = client.try_accept_admin();
    assert_eq!(
        res.unwrap_err().unwrap().unwrap().name(),
        "TimelockRequired"
    );

    // Test upgrade fails
    let dummy_wasm = BytesN::from_array(&env, &[9u8; 32]);
    let res = client.try_upgrade(&dummy_wasm);
    assert_eq!(
        res.unwrap_err().unwrap().unwrap().name(),
        "TimelockRequired"
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

    // Schedule Upgrade
    let dummy_wasm = BytesN::from_array(&env, &[9u8; 32]);
    let action_upgrade = TimelockAction::Upgrade(dummy_wasm.clone());
    let op_id_upgrade = client.timelock_schedule(&action_upgrade);

    // Fast forward beyond the delay
    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + 3600 + 1;
    });

    // Execute all
    client.timelock_execute(&op_id);
    client.timelock_execute(&op_id_upgrade);

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
    let relayer = Address::generate(&env);
    client.set_bridge_relayer(&relayer);
    assert_eq!(client.get_bridge_relayer(), Some(relayer));

    // Note: set_whitelist_root should technically fail because it has the guard,
    // but the issue requested documenting its behavior. We'll document the ones above which are truly open,
    // and below we'll test set_whitelist_root explicitly failing to show it is indeed guarded.
    let dummy_root = BytesN::from_array(&env, &[7u8; 32]);
    let res = client.try_set_whitelist_root(&dummy_root);
    assert_eq!(
        res.unwrap_err().unwrap().unwrap().name(),
        "TimelockRequired"
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

    let dummy_wasm = BytesN::from_array(&env, &[9u8; 32]);
    // Note: mock deployer allows upgrade call to succeed
    client.upgrade(&dummy_wasm);

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

/// Tests for the grace period and sweep functionality.
mod grace_period_tests {
    use super::*;
    use crate::storage_types::TimelockAction;
    use crate::timelock;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env,
    };

    fn setup_with_timelock(env: &Env) -> (StellarWrapContractClient<'_>, Address) {
        let contract_id = env.register(StellarWrapContract, ());
        let client = StellarWrapContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let pubkey = BytesN::from_array(env, &[1u8; 32]);

        client.initialize(&admin, &pubkey);
        env.mock_all_auths();
        client.enable_timelock(&timelock::MIN_DELAY);

        (client, admin)
    }

    #[test]
    fn test_execute_at_exact_grace_period_boundary_succeeds() {
        // Execute at exactly eta + GRACE_PERIOD should succeed (boundary is inclusive)
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        // Schedule an operation
        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let grace_expiry = eta.saturating_add(timelock::GRACE_PERIOD);

        // Advance to exactly the grace period boundary
        env.ledger().set_timestamp(grace_expiry);
        env.mock_all_auths();
        client.timelock_execute(&id);

        // Should have succeeded and removed from pending
        assert!(!client.timelock_pending().contains(&id));
        assert_eq!(client.get_admin().unwrap(), new_admin);
    }

    #[test]
    fn test_execute_just_inside_grace_period_succeeds() {
        // Execute at eta + GRACE_PERIOD - 1 should succeed
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let just_inside = eta.saturating_add(timelock::GRACE_PERIOD).saturating_sub(1);

        env.ledger().set_timestamp(just_inside);
        env.mock_all_auths();
        client.timelock_execute(&id);

        assert!(!client.timelock_pending().contains(&id));
        assert_eq!(client.get_admin().unwrap(), new_admin);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #54)")]
    fn test_execute_just_outside_grace_period_fails() {
        // Execute at eta + GRACE_PERIOD + 1 should fail with TimelockOperationExpired
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let just_outside = eta.saturating_add(timelock::GRACE_PERIOD).saturating_add(1);

        env.ledger().set_timestamp(just_outside);
        env.mock_all_auths();
        client.timelock_execute(&id);
    }

    #[test]
    fn test_sweep_expired_removes_from_pending() {
        // Anyone can sweep an expired operation from the pending list
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let after_grace = eta.saturating_add(timelock::GRACE_PERIOD).saturating_add(1);

        // Advance past grace period
        env.ledger().set_timestamp(after_grace);

        // Verify operation is still in pending (but expired)
        assert!(client.timelock_pending().contains(&id));

        // Clear auths to test permissionless sweep
        env.set_auths(&[]);
        client.timelock_sweep_expired(&id);

        // Operation should be removed from pending
        assert!(!client.timelock_pending().contains(&id));
        assert!(client.timelock_operation(&id).is_none());
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #55)")]
    fn test_sweep_before_grace_period_fails() {
        // Sweep before grace period ends should fail with TimelockOperationNotExpired
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let just_before_grace = eta.saturating_add(timelock::GRACE_PERIOD).saturating_sub(1);

        env.ledger().set_timestamp(just_before_grace);
        env.set_auths(&[]);
        client.timelock_sweep_expired(&id);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #41)")]
    fn test_sweep_nonexistent_operation_fails() {
        // Sweeping a non-existent operation should fail with TimelockOperationNotFound
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let dummy_id = BytesN::from_array(&env, &[99u8; 32]);
        env.set_auths(&[]);
        client.timelock_sweep_expired(&dummy_id);
    }

    #[test]
    fn test_sweep_expired_emits_event() {
        // Verify sweep event is emitted
        let env = Env::default();
        let (client, _admin) = setup_with_timelock(&env);

        let new_admin = Address::generate(&env);
        let action = TimelockAction::SetAdmin(new_admin.clone());
        let id = client.timelock_schedule(&action);

        let op = client.timelock_operation(&id).unwrap();
        let eta = op.eta;
        let after_grace = eta.saturating_add(timelock::GRACE_PERIOD).saturating_add(1);

        env.ledger().set_timestamp(after_grace);
        env.set_auths(&[]);
        client.timelock_sweep_expired(&id);

        let events = env.events().all();
        let mut sweep_found = false;
        for (_contract_id, topics, _data) in events.into_iter() {
            if topics.len() >= 2 {
                if topics.get(0).unwrap().into_val(&env) == symbol_short!("timelock").into_val(&env) {
                    let event_type = topics.get(1).unwrap().into_val(&env);
                    if event_type == symbol_short!("sweep").into_val(&env) {
                        sweep_found = true;
                    }
                }
            }
        }
        assert!(sweep_found, "sweep event not found");
    }
}
