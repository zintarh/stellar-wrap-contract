#![cfg(test)]

use crate::storage_types::{DataKey, TimelockAction, TimelockOperation};
use crate::{timelock, StellarWrapContract, StellarWrapContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env,
};

/// Registers + initializes the contract and enables the timelock with the
/// minimum legal delay, with all auth mocked.
fn setup_with_timelock(
    env: &Env,
    contract_id: &Address,
    client: &StellarWrapContractClient<'_>,
    admin: &Address,
    pubkey: &BytesN<32>,
) {
    client.initialize(admin, pubkey);
    client.enable_timelock(&timelock::MIN_DELAY);
    let _ = (env, contract_id);
}

fn root(env: &Env, b: u8) -> BytesN<32> {
    BytesN::from_array(env, &[b; 32])
}

/// Acceptance: Cancelling removes the id from timelock_pending() and
/// timelock_operation() returns None.
#[test]
fn test_cancel_removes_id_from_pending_and_operation() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 1);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    let action = TimelockAction::SetWhitelistRoot(root(&env, 11));
    let id = client.timelock_operation_id(&action);
    let scheduled = client.timelock_schedule(&action);

    assert_eq!(scheduled, id);
    assert_eq!(client.timelock_pending(), vec![&env, id.clone()]);
    assert!(client.timelock_operation(&id).is_some());

    client.timelock_cancel(&id);

    assert!(client.timelock_pending().is_empty());
    assert!(client.timelock_operation(&id).is_none());
}

/// Acceptance: Cancelling an unknown id fails with TimelockOperationNotFound (#41).
#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_cancel_unknown_id_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 2);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    // A real op sits in the queue so the failure is about the unknown id.
    client.timelock_schedule(&TimelockAction::SetWhitelistRoot(root(&env, 21)));
    client.timelock_cancel(&root(&env, 200));
}

/// Acceptance: With three operations queued, cancelling the middle one leaves
/// the other two intact and in order.
#[test]
fn test_cancel_middle_keeps_others_in_order() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 3);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    let a = client.timelock_schedule(&TimelockAction::SetWhitelistRoot(root(&env, 31)));
    let b = client.timelock_schedule(&TimelockAction::SetWhitelistRoot(root(&env, 32)));
    let c = client.timelock_schedule(&TimelockAction::SetWhitelistRoot(root(&env, 33)));

    assert_eq!(
        client.timelock_pending(),
        vec![&env, a.clone(), b.clone(), c.clone()]
    );

    client.timelock_cancel(&b);

    assert_eq!(client.timelock_pending(), vec![&env, a.clone(), c.clone()]);
    assert!(client.timelock_operation(&b).is_none());
    assert!(client.timelock_operation(&a).is_some());
    assert!(client.timelock_operation(&c).is_some());
}

/// Acceptance: Executing an operation removes it from the pending list.
#[test]
fn test_execute_removes_from_pending_list() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 4);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    let scheduled_root = root(&env, 41);
    let id = client.timelock_schedule(&TimelockAction::SetWhitelistRoot(scheduled_root.clone()));

    assert_eq!(client.timelock_pending(), vec![&env, id.clone()]);

    env.ledger().with_mut(|ledger| {
        ledger.timestamp += timelock::MIN_DELAY;
    });
    client.timelock_execute(&id);

    assert!(client.timelock_pending().is_empty());
    assert!(client.timelock_operation(&id).is_none());
    assert_eq!(client.get_whitelist_root(), Some(scheduled_root));
}

/// Executing before the ETA fails with TimelockNotReady (#40).
#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_execute_before_eta_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 5);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    let id = client.timelock_schedule(&TimelockAction::SetWhitelistRoot(root(&env, 51)));
    // ETA not reached.
    client.timelock_execute(&id);
}

/// Acceptance: After cancelling, the same action can be scheduled again (new ETA).
#[test]
fn test_reschedule_same_action_after_cancel() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 6);

    env.mock_all_auths();
    setup_with_timelock(&env, &contract_id, &client, &admin, &pubkey);

    let before = env.ledger().timestamp();
    let action = TimelockAction::SetWhitelistRoot(root(&env, 61));
    let id = client.timelock_schedule(&action);
    let first_op = client.timelock_operation(&id).unwrap();

    client.timelock_cancel(&id);
    assert!(client.timelock_operation(&id).is_none());

    // Advance the clock, then re-schedule the same action.
    env.ledger().with_mut(|ledger| {
        ledger.timestamp += 100;
    });
    let id2 = client.timelock_schedule(&action);
    let second_op = client.timelock_operation(&id2).unwrap();

    // Same deterministic id, new ETA.
    assert_eq!(id2, id);
    assert_eq!(second_op.eta, first_op.eta + 100);
    assert!(client.timelock_pending().contains(&id));
    let _ = before;
}

/// Acceptance: cancel requires admin authorization.
#[test]
#[should_panic]
fn test_cancel_requires_admin_authorization() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = root(&env, 7);
    let id = root(&env, 70);

    // Seed the contract as if admin had scheduled an operation, WITHOUT
    // enabling mock auths. A non-admin caller then attempts to cancel.
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::AdminPubKey, &pubkey);
        let op = TimelockOperation {
            action: TimelockAction::SetWhitelistRoot(root(&env, 71)),
            eta: 0,
            scheduled_at: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::TimelockOp(id.clone()), &op);
        let mut ids = soroban_sdk::Vec::new(&env);
        ids.push_back(id.clone());
        env.storage().instance().set(&DataKey::TimelockOps, &ids);
    });

    // Admin never authorized the cancel → require_auth panics.
    client.timelock_cancel(&id);
}
