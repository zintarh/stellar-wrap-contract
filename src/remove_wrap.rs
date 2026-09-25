use soroban_sdk::{Address, Env};

use crate::{storage_accounting, DataKey, WrapRecord};

// Shared helper: removes a wrap record and updates ownership indexes and
// storage accounting in the same way for both `burn_wrap` and `revoke_wrap`.
pub(crate) fn remove_wrap_record(e: &Env, user: &Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic!("WrapNotFound");
    }

    // Load record for possible checks by caller
    let _record: WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();

    // Remove the wrap entry and subtract estimated bytes for the wrap record
    e.storage().persistent().remove(&wrap_key);
    storage_accounting::sub_storage_bytes(&e, storage_accounting::estimate_wrap_bytes_new());

    // Update WrapPeriods, WrapCount, and LatestPeriod — mirrors previous revoke logic
    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    let wrap_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(e));

    let mut remaining_wrap_periods: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(e);
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
    } else {
        e.storage()
            .persistent()
            .set(&wrap_periods_key, &remaining_wrap_periods);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_periods_key, 17_280 * 365, 17_280 * 365);

        e.storage().persistent().set(&count_key, &new_count);
        e.storage()
            .persistent()
            .extend_ttl(&count_key, 17_280 * 365, 17_280 * 365);

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
            .extend_ttl(&latest_key, 17_280 * 365, 17_280 * 365);
    }

    // Keep UserPeriods in sync (legacy index used by get_wraps / get_latest_wrap)
    let user_periods_key = DataKey::UserPeriods(user.clone());
    let user_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&user_periods_key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(e));

    let mut remaining_user_periods: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(e);
    for p in user_periods.iter() {
        if p != period {
            remaining_user_periods.push_back(p);
        }
    }

    if remaining_user_periods.is_empty() {
        e.storage().persistent().remove(&user_periods_key);
    } else {
        e.storage()
            .persistent()
            .set(&user_periods_key, &remaining_user_periods);
        e.storage()
            .persistent()
            .extend_ttl(&user_periods_key, 17_280 * 365, 17_280 * 365);
    }
}
