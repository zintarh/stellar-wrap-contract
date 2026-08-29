//! Timelock controller for privileged (admin) actions.
//!
//! Once [`enable`] has been called, sensitive admin mutations can no longer be
//! applied in a single transaction. They must be `schedule`d, wait out the
//! configured delay, and only then be `execute`d — giving users a public,
//! on-chain window in which to observe a pending admin change and react to it.
//!
//! See `docs/timelock.md` for the full architecture and operator runbook.

use soroban_sdk::{panic_with_error, symbol_short, xdr::ToXdr, Bytes, BytesN, Env, Vec};

use crate::storage_types::{TimelockAction, TimelockOperation};
use crate::{ContractError, DataKey};

/// Smallest delay the timelock accepts (1 hour). A shorter window would not
/// give observers a realistic chance to react.
pub const MIN_DELAY: u64 = 3_600;
/// Largest delay the timelock accepts (30 days). Prevents bricking the contract
/// with an effectively infinite delay.
pub const MAX_DELAY: u64 = 30 * 24 * 3_600;
/// Expiration grace period (14 days). An operation that is not executed within
/// `eta + GRACE_PERIOD` expires and can no longer be executed.
pub const GRACE_PERIOD: u64 = 14 * 24 * 3_600;

/// Persistent TTL for scheduled operations (~1 year in ledgers), matching the
/// TTL used for wrap records elsewhere in the contract.
const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Returns the configured delay in seconds, or `None` while the timelock is
/// disabled.
pub(crate) fn delay(e: &Env) -> Option<u64> {
    e.storage().instance().get(&DataKey::TimelockDelay)
}

/// Whether the timelock is enabled (and therefore enforced).
pub(crate) fn is_enabled(e: &Env) -> bool {
    delay(e).is_some()
}

/// Guard for the direct (non-timelocked) admin paths.
///
/// Direct calls such as `update_admin` and `upgrade` stay available until the
/// timelock is enabled; afterwards they panic so the only route to a privileged
/// mutation is `schedule` + `execute`.
pub(crate) fn require_direct_call_allowed(e: &Env) {
    if is_enabled(e) {
        panic_with_error!(e, ContractError::TimelockRequired);
    }
}

fn validate_delay(e: &Env, seconds: u64) {
    if !(MIN_DELAY..=MAX_DELAY).contains(&seconds) {
        panic_with_error!(e, ContractError::InvalidTimelockDelay);
    }
}

/// Admin-only, one-way switch that turns the timelock on with `delay_seconds`.
///
/// It cannot be disabled or shortened afterwards without going through the
/// timelock itself (`TimelockAction::SetTimelockDelay`), which is exactly the
/// guarantee the controller is meant to provide.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn enable(e: Env, delay_seconds: u64) {
    crate::admin::read_admin(&e).require_auth();

    if is_enabled(&e) {
        panic_with_error!(e, ContractError::TimelockAlreadyEnabled);
    }
    validate_delay(&e, delay_seconds);

    e.storage()
        .instance()
        .set(&DataKey::TimelockDelay, &delay_seconds);
    e.events().publish(
        (symbol_short!("timelock"), symbol_short!("enabled")),
        delay_seconds,
    );
}

/// Deterministic id of an action: SHA-256 over a variant tag and the XDR
/// encoding of the variant payload.
///
/// The id deliberately excludes the ETA so the same action cannot be queued
/// twice concurrently, and so off-chain tooling can predict the id of an
/// operation before it is scheduled.
pub(crate) fn operation_id(e: &Env, action: &TimelockAction) -> BytesN<32> {
    let mut data = Bytes::new(e);
    match action {
        TimelockAction::SetAdmin(addr) => {
            data.append(&Bytes::from_array(e, &[1u8]));
            data.append(&addr.clone().to_xdr(e));
        }
        TimelockAction::SetAdminPubKey(key) => {
            data.append(&Bytes::from_array(e, &[2u8]));
            data.append(&key.clone().to_xdr(e));
        }
        TimelockAction::Upgrade(hash) => {
            data.append(&Bytes::from_array(e, &[3u8]));
            data.append(&hash.clone().to_xdr(e));
        }
        TimelockAction::SetWhitelistRoot(root) => {
            data.append(&Bytes::from_array(e, &[4u8]));
            data.append(&root.clone().to_xdr(e));
        }
        TimelockAction::SetTimelockDelay(seconds) => {
            data.append(&Bytes::from_array(e, &[5u8]));
            data.append(&(*seconds).to_xdr(e));
        }
    }
    let hash = e.crypto().sha256(&data);
    BytesN::from_array(e, &hash.to_array())
}

fn read_op_ids(e: &Env) -> Vec<BytesN<32>> {
    e.storage()
        .instance()
        .get(&DataKey::TimelockOps)
        .unwrap_or_else(|| Vec::new(e))
}

fn remove_op(e: &Env, id: &BytesN<32>) {
    e.storage()
        .persistent()
        .remove(&DataKey::TimelockOp(id.clone()));

    let ids = read_op_ids(e);
    let mut remaining = Vec::new(e);
    for existing in ids.iter() {
        if existing != *id {
            remaining.push_back(existing);
        }
    }
    e.storage()
        .instance()
        .set(&DataKey::TimelockOps, &remaining);
}

/// Admin-only: queue `action` for execution once the delay has elapsed.
///
/// Returns the operation id needed by [`execute`] and [`cancel`].
///
/// # Panics
/// - [`ContractError::NotInitialized`] if there is no admin.
/// - [`ContractError::InvalidTimelockDelay`] if the timelock is not enabled.
/// - [`ContractError::TimelockOperationExists`] if the same action is queued.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn schedule(e: Env, action: TimelockAction) -> BytesN<32> {
    crate::admin::read_admin(&e).require_auth();

    let delay_seconds =
        delay(&e).unwrap_or_else(|| panic_with_error!(e, ContractError::InvalidTimelockDelay));

    // A delay change is validated at schedule time so an invalid value can
    // never sit in the queue waiting to brick the controller.
    if let TimelockAction::SetTimelockDelay(seconds) = &action {
        validate_delay(&e, *seconds);
    }

    let id = operation_id(&e, &action);
    let key = DataKey::TimelockOp(id.clone());
    if e.storage().persistent().has(&key) {
        panic_with_error!(e, ContractError::TimelockOperationExists);
    }

    let now = e.ledger().timestamp();
    let op = TimelockOperation {
        action,
        eta: now.saturating_add(delay_seconds),
        scheduled_at: now,
    };

    e.storage().persistent().set(&key, &op);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    let mut ids = read_op_ids(&e);
    ids.push_back(id.clone());
    e.storage().instance().set(&DataKey::TimelockOps, &ids);

    e.events().publish(
        (symbol_short!("timelock"), symbol_short!("sched")),
        (id.clone(), op.eta),
    );

    id
}

/// Admin-only: drop a scheduled operation before it executes.
///
/// # Panics
/// - [`ContractError::TimelockOperationNotFound`] if `id` is not queued.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn cancel(e: Env, id: BytesN<32>) {
    crate::admin::read_admin(&e).require_auth();

    if !e
        .storage()
        .persistent()
        .has(&DataKey::TimelockOp(id.clone()))
    {
        panic_with_error!(e, ContractError::TimelockOperationNotFound);
    }

    remove_op(&e, &id);
    e.events()
        .publish((symbol_short!("timelock"), symbol_short!("cancel")), id);
}

/// Admin-only: apply a scheduled operation whose ETA has passed.
///
/// The operation is removed from the queue before its effect is applied, so an
/// execution can never be replayed.
///
/// # Panics
/// - [`ContractError::TimelockOperationNotFound`] if `id` is not queued.
/// - [`ContractError::TimelockNotReady`] if the ETA has not been reached.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn execute(e: Env, id: BytesN<32>) {
    let admin = crate::admin::read_admin(&e);
    admin.require_auth();

    let op: TimelockOperation = e
        .storage()
        .persistent()
        .get(&DataKey::TimelockOp(id.clone()))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::TimelockOperationNotFound));

    let now = e.ledger().timestamp();
    if now < op.eta {
        panic_with_error!(e, ContractError::TimelockNotReady);
    }
    if now > op.eta.saturating_add(GRACE_PERIOD) {
        panic_with_error!(e, ContractError::TimelockOperationExpired);
    }

    remove_op(&e, &id);

    match op.action {
        TimelockAction::SetAdmin(new_admin) => {
            e.storage().instance().set(&DataKey::Admin, &new_admin);
            // A queued handover supersedes any in-flight two-step proposal.
            e.storage().instance().remove(&DataKey::PendingAdmin);
            e.events().publish(
                (symbol_short!("admin"), symbol_short!("updated")),
                (admin, new_admin),
            );
        }
        TimelockAction::SetAdminPubKey(key) => {
            e.storage().instance().set(&DataKey::AdminPubKey, &key);
        }
        TimelockAction::SetWhitelistRoot(root) => {
            e.storage().instance().set(&DataKey::WhitelistRoot, &root);
        }
        TimelockAction::SetTimelockDelay(seconds) => {
            validate_delay(&e, seconds);
            e.storage()
                .instance()
                .set(&DataKey::TimelockDelay, &seconds);
        }
        TimelockAction::Upgrade(wasm_hash) => {
            e.events()
                .publish((symbol_short!("upgrade"),), wasm_hash.clone());
            e.deployer().update_current_contract_wasm(wasm_hash);
        }
    }

    e.events()
        .publish((symbol_short!("timelock"), symbol_short!("exec")), id);
}

/// Remove an expired timelock operation from storage and the pending list.
///
/// Callable by anyone once `ledger.timestamp > op.eta + GRACE_PERIOD`.
///
/// # Panics
/// - [`ContractError::TimelockOperationNotFound`] if `id` is not queued.
/// - [`ContractError::TimelockOperationNotExpired`] if the grace period has not elapsed.
pub(crate) fn sweep_expired(e: Env, id: BytesN<32>) {
    let op: TimelockOperation = e
        .storage()
        .persistent()
        .get(&DataKey::TimelockOp(id.clone()))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::TimelockOperationNotFound));

    let now = e.ledger().timestamp();
    if now <= op.eta.saturating_add(GRACE_PERIOD) {
        panic_with_error!(e, ContractError::TimelockOperationNotExpired);
    }

    remove_op(&e, &id);
    e.events()
        .publish((symbol_short!("timelock"), symbol_short!("sweep")), id);
}

/// Return a scheduled operation by id, or `None` if it is not queued.
pub(crate) fn get_operation(e: &Env, id: BytesN<32>) -> Option<TimelockOperation> {
    e.storage().persistent().get(&DataKey::TimelockOp(id))
}

/// Return the ids of every currently scheduled operation.
pub(crate) fn pending_operations(e: &Env) -> Vec<BytesN<32>> {
    read_op_ids(e)
}
