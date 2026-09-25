use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

use crate::storage_accounting;
use crate::{ContractError, DataKey, WrapRecord, WrapState};
use crate::remove_wrap::remove_wrap_record;

/// Burns (permanently deletes) a wrap record owned by the caller.
///
/// # Authorization
/// Only the wrap owner can burn their own wrap. The caller must provide
/// authorization via `require_auth()` and the wrap must exist in storage.
///
/// # Arguments
/// * `user` - The address of the wrap owner
/// * `period` - The period (YYYYMM) of the wrap to delete
///
/// # Errors
/// * `WrapNotFound` if the wrap_id (user, period pair) does not exist in storage
/// * `Unauthorized` if caller is not the wrap owner
/// * `InvalidStateTransition` if the wrap is in the Bridged state
///
/// # Side Effects
/// 1. Removes the wrap record from persistent storage (reclaiming its
///    accounted storage bytes)
/// 2. Decrements the user's wrap count (removing key when count reaches zero)
/// 3. Updates WrapPeriods ownership index (removing key when empty)
/// 4. Updates UserPeriods list (removing key when empty)
/// 5. Updates LatestPeriod from remaining WrapPeriods, or removes key when empty
/// 6. Decrements the global TotalWrapCount
/// 7. Records the state-change timestamp via update_last_updated
/// 8. Emits a `burn` event after deletion
///
/// # Notes
/// Once burned, the wrap_id is freed and the record cannot be recovered.
/// The user can later mint a new wrap for the same period if desired.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn burn_wrap(e: Env, user: Address, period: u64) {
    // Require auth FIRST — verify caller is the owner.
    user.require_auth();

    // Remove the wrap record and unwind all per-user bookkeeping.
    // The helper also enforces WrapNotFound and the Bridged-state guard.
    crate::wrap_record_helpers::remove_wrap_record(&e, &user, period);

    let record: WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();
    if record.fsm.state == WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    // 3. Delete the wrap record from storage and reclaim its accounted bytes
    //    (mirrors revoke_wrap — see tests/storage_fee.rs::burn_matches_revoke_delta)
    e.storage().persistent().remove(&wrap_key);
    storage_accounting::sub_storage_bytes(&e, storage_accounting::estimate_wrap_bytes_new());

    // 4. Update WrapPeriods ownership index (used by transfer_wrap / read_periods).
    //    This MUST stay in sync with WrapCount or every subsequent transfer panics.
    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    let wrap_periods: Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)
        .unwrap_or_else(|| Vec::new(&e));

    // Remove the burned period from the index
    let mut remaining_wrap_periods: Vec<u64> = Vec::new(&e);
    for p in wrap_periods.iter() {
        if p != period {
            remaining_wrap_periods.push_back(p);
        }
    }

    // 5. Compute new WrapCount from the filtered WrapPeriods length
    let new_count = remaining_wrap_periods.len();
    let count_key = DataKey::WrapCount(user.clone());

    // 6. Persist WrapPeriods / WrapCount / LatestPeriod atomically
    if remaining_wrap_periods.is_empty() {
        e.storage().persistent().remove(&wrap_periods_key);
        e.storage().persistent().remove(&count_key);
        e.storage()
            .persistent()
            .remove(&DataKey::LatestPeriod(user.clone()));
        storage_accounting::sub_storage_bytes(
            &e,
            storage_accounting::estimate_wrapcount_bytes_new(),
        );
        storage_accounting::sub_storage_bytes(
            &e,
            storage_accounting::estimate_latest_bytes_new(),
        );
    } else {
        e.storage()
            .persistent()
            .set(&wrap_periods_key, &remaining_wrap_periods);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        e.storage().persistent().set(&count_key, &new_count);
        e.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        // Recompute LatestPeriod from the remaining periods
        let mut latest = remaining_wrap_periods.get(0).unwrap();
        for p in remaining_wrap_periods.iter() {
            if p > latest {
                latest = p;
            }
        }
        let latest_key = DataKey::LatestPeriod(user.clone());
        e.storage().persistent().set(&latest_key, &latest);
        e.storage()
            .persistent()
            .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    // 7. Also keep UserPeriods in sync (legacy index used by get_wraps / get_latest_wrap)
    let user_periods_key = DataKey::UserPeriods(user.clone());
    let user_periods: Vec<u64> = e
        .storage()
        .persistent()
        .get(&user_periods_key)
        .unwrap_or_else(|| Vec::new(&e));

    let mut remaining_user_periods: Vec<u64> = Vec::new(&e);
    for p in user_periods.iter() {
        if p != period {
            remaining_user_periods.push_back(p);
        }
    }

    if remaining_user_periods.is_empty() {
        e.storage().persistent().remove(&user_periods_key);
        storage_accounting::sub_storage_bytes(
            &e,
            storage_accounting::estimate_userperiods_bytes_new(),
        );
    } else {
        e.storage()
            .persistent()
            .set(&user_periods_key, &remaining_user_periods);
        e.storage()
            .persistent()
            .extend_ttl(&user_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    // 8. Decrement global total wrap count (live wrap count)
    let total_key = DataKey::TotalWrapCount;
    let total: u32 = e.storage().persistent().get(&total_key).unwrap_or(0);
    e.storage().persistent().set(&total_key, &total.saturating_sub(1));
    e.storage().persistent().extend_ttl(&total_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    // 9. Emit burn event AFTER all state mutations
    e.events()
        .publish((symbol_short!("burn"), user.clone(), period), user);
}