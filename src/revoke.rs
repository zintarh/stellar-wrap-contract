use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env, Vec};

use crate::constants::TTL_ONE_YEAR;
use crate::{storage_accounting, ContractError, DataKey};

/// Revokes an existing wrap record for the given user and period.
/// This function decrements the global `TotalWrapCount`, which tracks the
/// number of currently live wraps.
///
/// #Authorization
/// Only the contract admin can revoke wraps. The caller must provide admin
/// authorization via `require_auth(`.
///
/// #reason_hash
/// An optional SHA-256 hash of an off-chain revocation reason. This allows
/// auditors to link a revocation to off-chain evidence (e.g., a governance
/// vote, dispute resolution, or compliance action) without exposing private
/// data on-chain. Pass a zero-filled `BytesN&32> to omit the reason.
///
/// #Privacy Considerations
/// - The `reason_hash` is a one-way hash. It cannot be reversed to reveal the
///   original reason, preserving confidentiality of sensitive decisions.
/// - Evidence handling: the off-chain reason should be stored in a verifiable
///   location (e.g., a signed document, IPTS, or a governance log) so that
///   auditors can recompute the hash and confirm it matches the on-chain value.
/// - If no reason is provided (all-zero hash), the event still emits for
///   transparency, but without a link to off-chain evidence.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn revoke_wrap(e: Env, user: Address, period: u64, reason_hash: BytesN<32>) {
    let admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    admin.require_auth();

    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapNotFound);
    }

    // Remove the wrap entry and subtract estimated bytes
    e.storage().persistent().remove(&wrap_key);
    storage_accounting::sub_storage_bytes(&e, storage_accounting::estimate_wrap_bytes_new());

    // Update WrapPeriods, WrapCount, and LatestPeriod atomically — mirrors
    // burn_wrap so the invariant WrapCount == WrapPeriods.len() always holds.
    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    let wrap_periods: Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)
        .unwrap_or_else(|| Vec::new(&e));

    let mut remaining_wrap_periods: Vec<u64> = Vec::new(&e);
    for p in wrap_periods.iter() {
        if p != period {
            remaining_wrap_periods.push_back(p);
        }
    }

    let count_key = DataKey::WrapCount(user.clone());
    let new_count = remaining_wrap_periods.len();

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

        // Recompute LatestPeriod from remaining periods
        let mut latest: u64 = 0;
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

    // Keep UserPeriods in sync (legacy index used by get_wraps / get_latest_wrap)
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

    // Increment TotalRevoked with overflow protection (instance storage).
    let total_revoked_key = DataKey::TotalRevoked;
    let current_total: u64 = e.storage().instance().get(&total_revoked_key).unwrap_or(0);
    let next_total = current_total
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(&e, ContractError::ArithmeticOverflow));
    e.storage().instance().set(&total_revoked_key, &next_total);

    e.events()
        .publish((symbol_short!("revoke"), user, period), reason_hash);
}