//! Shared helpers for removing a wrap record and unwinding all associated
//! per-user state. Both `revoke_wrap` and `burn_wrap` (and the source-side of
//! `transfer_wrap`) delegate to `remove_wrap_record` so that every entrypoint
//! produces an identical state delta for the same `(user, period)`.

use soroban_sdk::{panic_with_error, Address, Env};

use crate::{storage_accounting, ContractError, DataKey};
use crate::storage_types::WrapState;

const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Remove the wrap record for `(user, period)` and unwind every piece of
/// per-user bookkeeping:
///
/// 1. Verify the wrap exists — panic with `WrapNotFound` if not.
/// 2. Remove `DataKey::Wrap(user, period)` from persistent storage.
/// 3. Subtract estimated wrap bytes from the storage-accounting counter.
/// 4. Decrement `DataKey::WrapCount(user)`.
///    - If the count reaches zero, remove the key and subtract the
///      corresponding accounting bytes.
/// 5. Clear `DataKey::LatestPeriod(user)` when the removed period was the
///    stored latest (no recompute; let the next mint set a new one).
/// 6. Remove `period` from `DataKey::UserPeriods(user)`.
///    - Remove the key entirely when the list becomes empty.
/// 7. Remove `period` from `DataKey::WrapPeriods(user)` (the transfer index).
///    - Remove the key entirely when the list becomes empty.
/// 8. Decrement the global `DataKey::TotalWrapCount` counter.
/// 9. Call `mint::update_last_updated` to record the state-change timestamp.
///
/// The caller is responsible for:
/// - Authorization (admin check for revoke, owner check for burn).
/// - Emitting the appropriate event.
/// - Any operation-specific bookkeeping (e.g. `TotalRevoked` for revoke).
pub(crate) fn remove_wrap_record(e: &Env, user: &Address, period: u64) {
    // 1. Guard: wrap must exist.
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapNotFound);
    }

    // Guard: cannot remove a bridged-out record.
    let record: crate::WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();
    if record.fsm.state == WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    // 2. Remove the wrap record.
    e.storage().persistent().remove(&wrap_key);

    // 3. Storage accounting: subtract the wrap entry bytes.
    storage_accounting::sub_storage_bytes(e, storage_accounting::estimate_wrap_bytes_new());

    // 4. Decrement WrapCount.
    let count_key = DataKey::WrapCount(user.clone());
    let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
    if current_count > 0 {
        let next_count = current_count - 1;
        if next_count == 0 {
            e.storage().persistent().remove(&count_key);
            storage_accounting::sub_storage_bytes(
                e,
                storage_accounting::estimate_wrapcount_bytes_new(),
            );
        } else {
            e.storage().persistent().set(&count_key, &next_count);
            e.storage()
                .persistent()
                .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    }

    // 5. Clear LatestPeriod if it pointed at the removed period.
    let latest_key = DataKey::LatestPeriod(user.clone());
    let current_latest: Option<u64> = e.storage().persistent().get(&latest_key);
    if current_latest == Some(period) {
        e.storage().persistent().remove(&latest_key);
    }

    // 6. Remove `period` from UserPeriods.
    remove_from_user_periods(e, user, period);

    // 7. Remove `period` from WrapPeriods (transfer index).
    remove_from_wrap_periods(e, user, period);

    // 8. Decrement TotalWrapCount.
    let total_key = DataKey::TotalWrapCount;
    let current_total: u32 = e.storage().persistent().get(&total_key).unwrap_or(0);
    if current_total > 0 {
        let next_total = current_total - 1;
        e.storage().persistent().set(&total_key, &next_total);
        e.storage()
            .persistent()
            .extend_ttl(&total_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    // 9. Update the last-updated timestamp.
    crate::mint::update_last_updated(e, user);
}

/// Remove `period` from `DataKey::UserPeriods(user)`.
/// Removes the entire key when the list becomes empty.
fn remove_from_user_periods(e: &Env, user: &Address, period: u64) {
    let key = DataKey::UserPeriods(user.clone());
    let mut periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(soroban_sdk::Vec::new(e));

    let mut found_index: Option<u32> = None;
    for (i, p) in periods.iter().enumerate() {
        if p == period {
            found_index = Some(i as u32);
            break;
        }
    }

    if let Some(idx) = found_index {
        periods.remove(idx);
        if periods.is_empty() {
            e.storage().persistent().remove(&key);
        } else {
            e.storage().persistent().set(&key, &periods);
            e.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    }
}

/// Remove `period` from `DataKey::WrapPeriods(user)` (the transfer index).
/// Removes the entire key when the list becomes empty.
pub(crate) fn remove_from_wrap_periods(e: &Env, user: &Address, period: u64) {
    let key = DataKey::WrapPeriods(user.clone());
    let mut periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(soroban_sdk::Vec::new(e));

    let mut found_index: Option<u32> = None;
    for (i, p) in periods.iter().enumerate() {
        if p == period {
            found_index = Some(i as u32);
            break;
        }
    }

    if let Some(idx) = found_index {
        periods.remove(idx);
        if periods.is_empty() {
            e.storage().persistent().remove(&key);
        } else {
            e.storage().persistent().set(&key, &periods);
            e.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    }
}
