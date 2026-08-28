//! Shared record-removal logic used by both `revoke_wrap` and `burn_wrap`.
//!
//! # Why a single helper?
//! The two entrypoints share every state mutation except auth and event
//! emission. Centralising the mutations here eliminates the divergence
//! described in issue #720 and ensures both paths always produce identical
//! storage deltas for the same record.
//!
//! # What this helper owns
//! 1. Remove `Wrap(user, period)` from persistent storage and subtract the
//!    estimated storage bytes.
//! 2. Remove the period from the canonical `WrapPeriods` index and call
//!    `write_owner_state` to keep `WrapCount` and `LatestPeriod` consistent.
//! 3. Remove the period from `UserPeriods` if present (backward-compatibility
//!    clean-up for records written before the `WrapPeriods` index existed).
//! 4. Call `update_last_updated` so the user's activity timestamp is always
//!    refreshed on any removal path.
//!
//! # What this helper does NOT own
//! * Authorization — callers are responsible for verifying who may act.
//! * Event emission — each entrypoint emits its own domain-specific event.
//! * `TotalRevoked` — that counter is semantically a revocation metric, not a
//!   generic removal metric, so `revoke_wrap` owns it.

use soroban_sdk::{panic_with_error, Address, Env, Vec};

use crate::storage_accounting;
use crate::transfer::{remove_period, write_owner_state};
use crate::{ContractError, DataKey};

/// Remove a wrap record and unwind all associated per-user state.
///
/// Panics with `WrapNotFound` if no record exists for `(user, period)`.
///
/// State mutations (in order):
/// 1. Delete `Wrap(user, period)` + subtract storage bytes.
/// 2. Update the `WrapPeriods` index → drives `WrapCount` + `LatestPeriod`.
/// 3. Clean up the legacy `UserPeriods` list if it contains the period.
/// 4. Refresh `LastUpdated` for the user.
pub(crate) fn remove_wrap_record(e: &Env, user: &Address, period: u64) {
    // ── 1. Guard: record must exist ───────────────────────────────────────
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapNotFound);
    }

    // ── 2. Delete the wrap record + storage accounting ────────────────────
    e.storage().persistent().remove(&wrap_key);
    storage_accounting::sub_storage_bytes(e, storage_accounting::estimate_wrap_bytes_new());

    // ── 3. Update canonical WrapPeriods index ─────────────────────────────
    // `write_owner_state` maintains WrapCount and LatestPeriod in lock-step
    // with the periods vector, so we only need to remove the period here and
    // hand the resulting slice back to it.
    let periods_key = DataKey::WrapPeriods(user.clone());
    let current_periods: Vec<u64> = e
        .storage()
        .persistent()
        .get(&periods_key)
        .unwrap_or_else(|| Vec::new(e));

    let updated_periods = remove_period(e, &current_periods, period);
    write_owner_state(e, user, &updated_periods);

    // ── 4. Clean up legacy UserPeriods list (backward-compat) ─────────────
    // Records written before the WrapPeriods index was introduced were tracked
    // in UserPeriods instead. Remove the entry if it is still present so that
    // old and new records both leave clean storage after removal.
    let user_periods_key = DataKey::UserPeriods(user.clone());
    if let Some(mut user_periods) = e
        .storage()
        .persistent()
        .get::<DataKey, Vec<u64>>(&user_periods_key)
    {
        // Find and remove the period from the legacy list.
        let mut found_index: Option<u32> = None;
        for (i, p) in user_periods.iter().enumerate() {
            if p == period {
                found_index = Some(i as u32);
                break;
            }
        }
        if let Some(idx) = found_index {
            user_periods.remove(idx);
            if user_periods.is_empty() {
                e.storage().persistent().remove(&user_periods_key);
            } else {
                e.storage()
                    .persistent()
                    .set(&user_periods_key, &user_periods);
            }
        }
    }

    // ── 5. Refresh last-updated timestamp ─────────────────────────────────
    crate::mint::update_last_updated(e, user);
}
