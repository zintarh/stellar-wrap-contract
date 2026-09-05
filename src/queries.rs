use soroban_sdk::{Address, Bytes, BytesN, Env, String};

use crate::{ContractHealth, DataKey, InvariantReport, TransferFeeConfig, WrapRecord};

pub(crate) fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
    e.storage().persistent().get(&DataKey::Wrap(user, period))
}

pub(crate) fn get_mint_timestamp(e: Env, user: Address, period: u64) -> Option<u64> {
    let wrap: Option<WrapRecord> = e.storage().persistent().get(&DataKey::Wrap(user, period));
    wrap.map(|r| r.timestamp)
}

/// Return the ledger timestamp of the user's most recent state change via a
/// successful mint or revoke, or `None` if the user has never minted or had a
/// wrap revoked.
pub(crate) fn get_last_updated(e: Env, user: Address) -> Option<u64> {
    e.storage().persistent().get(&DataKey::LastUpdated(user))
}

/// Returns the number of active wrap records for the user.
///
/// Under the hood, this retrieves the `u32` wrap record counter (`DataKey::WrapCount`)
/// and casts it to `i128` to satisfy the standard token interface return signature.
/// The return value represents a count of discrete wrap records, not a fungible token balance.
pub(crate) fn balance_of(e: Env, user: Address) -> i128 {
    e.storage()
        .persistent()
        .get::<_, u32>(&DataKey::WrapCount(user))
        .unwrap_or(0) as i128
}

pub(crate) fn total_wrap_count(e: Env) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::TotalWrapCount)
        .unwrap_or(0)
}

pub(crate) fn verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool {
    let wrap: Option<WrapRecord> = e.storage().persistent().get(&DataKey::Wrap(user, period));
    wrap.is_some_and(|record| {
        let computed_hash = e.crypto().sha256(&data);
        let computed_hash = BytesN::from_array(&e, &computed_hash.to_array());
        record.data_hash == computed_hash
    })
}

pub(crate) fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
    let latest_key = DataKey::LatestPeriod(user.clone());
    if let Some(period) = e.storage().persistent().get::<_, u64>(&latest_key) {
        if let Some(wrap) = e
            .storage()
            .persistent()
            .get(&DataKey::Wrap(user.clone(), period))
        {
            return Some(wrap);
        }
    }

    // Fallback: Scan UserPeriods to find the latest wrap.
    // This is kept as a legacy-data safety net in case LatestPeriod becomes stale
    // due to corruption or migration issues. Under normal operation, LatestPeriod
    // should always be recomputed correctly during revoke/burn, so this fallback
    // should not be triggered.
    let periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::UserPeriods(user.clone()))?;
    let mut latest: Option<WrapRecord> = None;

    for index in 0..periods.len() {
        if let Some(period) = periods.get(index) {
            if let Some(wrap) = e
                .storage()
                .persistent()
                .get::<_, WrapRecord>(&DataKey::Wrap(user.clone(), period))
            {
                let is_newer = match latest.as_ref() {
                    None => true,
                    Some(current) => wrap.period > current.period,
                };
                if is_newer {
                    latest = Some(wrap);
                }
            }
        }
    }

    latest
}

pub(crate) fn get_wraps(
    e: Env,
    user: Address,
    start: u32,
    limit: u32,
) -> soroban_sdk::Vec<WrapRecord> {
    let mut results = soroban_sdk::Vec::new(&e);
    let user_periods_key = DataKey::UserPeriods(user.clone());

    if let Some(periods) = e
        .storage()
        .persistent()
        .get::<_, soroban_sdk::Vec<u64>>(&user_periods_key)
    {
        let len = periods.len();
        if start < len {
            let end = core::cmp::min(start.saturating_add(limit), len);
            for i in start..end {
                if let Some(period) = periods.get(i) {
                    if let Some(wrap) = e
                        .storage()
                        .persistent()
                        .get(&DataKey::Wrap(user.clone(), period))
                    {
                        results.push_back(wrap);
                    }
                }
            }
        }
    }

    results
}

/// Returns every wrap record owned by `user` in a single call.
///
/// This is a convenience wrapper around [`get_wraps`] that requests all records
/// without pagination. It is intended for bounded queries of at most
/// [`MAX_QUERY_RESULTS`] (200) records. Callers with larger datasets should use
/// the paginated [`get_wraps`] instead to stay within Soroban resource limits.
pub(crate) fn get_all_wraps_for_user(e: Env, user: Address) -> soroban_sdk::Vec<WrapRecord> {
    // Fetch all wraps up to the maximum query result limit.
    get_wraps(e, user, 0, MAX_QUERY_RESULTS)
}

/// Return the configured transfer-fee configuration, or `None` if unset.
pub(crate) fn get_transfer_fee(e: Env) -> Option<TransferFeeConfig> {
    e.storage().instance().get(&DataKey::TransferFee)
}

pub(crate) fn health(e: Env) -> ContractHealth {
    let has_admin = e.storage().instance().has(&DataKey::Admin);
    let has_signing_key = e.storage().instance().has(&DataKey::AdminPubKey);

    ContractHealth {
        initialized: has_admin,
        has_admin,
        has_signing_key,
    }
}

pub(crate) fn get_admin(e: Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::Admin)
}

/// Return the configured Ed25519 admin public key, or `None` before `initialize`.
///
/// Operators use this to confirm which off-chain signing key is live without
/// inspecting raw instance storage. The value is a public verification key only;
/// it does not reveal private key material. Prefer this over reading storage
/// directly when building ops/monitoring tooling.
pub(crate) fn get_admin_pubkey(e: Env) -> Option<BytesN<32>> {
    e.storage().instance().get(&DataKey::AdminPubKey)
}

/// Return the contract semantic version string (`MAJOR.MINOR.PATCH`).
///
/// Keep this in sync with `Cargo.toml` package version. Bump it in the same
/// release that ships a WASM upgrade so clients can detect which interface they
/// are talking to after `upgrade()`.
pub(crate) fn version(e: Env) -> String {
    String::from_str(&e, "0.1.0")
}

/// Cheap existence check for `(user, period)` without loading a `WrapRecord`.
///
/// Prefer `has_wrap` when callers only need a boolean (indexing, gating UI).
/// Use `get_wrap` when the full record (timestamp, archetype, hash, FSM) is
/// required.
pub(crate) fn has_wrap(e: Env, user: Address, period: u64) -> bool {
    e.storage().persistent().has(&DataKey::Wrap(user, period))
}

pub(crate) fn total_revoked(e: Env) -> u64 {
    e.storage()
        .instance()
        .get::<_, u64>(&DataKey::TotalRevoked)
        .unwrap_or(0)
}

pub(crate) fn name(e: Env) -> String {
    e.storage()
        .temporary()
        .get(&DataKey::Name)
        .unwrap_or_else(|| String::from_str(&e, "Stellar Wrap Registry"))
}

pub(crate) fn symbol(e: Env) -> String {
    e.storage()
        .temporary()
        .get(&DataKey::Symbol)
        .unwrap_or_else(|| String::from_str(&e, "WRAP"))
}

/// Returns `0` because wrap records represent discrete, indivisible registry entries with
/// no fractional units.
pub(crate) fn decimals(_e: Env) -> u32 {
    0
}

pub(crate) fn contract_version(e: Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::ContractVersion)
        .unwrap_or(0)
}

pub const MAX_QUERY_RESULTS: u32 = 200;

pub(crate) fn check_user_invariants(e: Env, user: Address) -> InvariantReport {
    let wrap_count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::WrapCount(user.clone()))
        .unwrap_or(0);

    let user_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::UserPeriods(user.clone()))
        .unwrap_or_else(|| soroban_sdk::Vec::new(&e));
    let user_periods_len = user_periods.len();

    let wrap_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::WrapPeriods(user.clone()))
        .unwrap_or_else(|| soroban_sdk::Vec::new(&e));
    let wrap_periods_len = wrap_periods.len();

    let latest_period: Option<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::LatestPeriod(user.clone()));

    let mut max_user_period: Option<u64> = None;
    let mut live_wraps_found = 0;

    let scan_len = core::cmp::min(user_periods_len, MAX_QUERY_RESULTS);
    for i in 0..scan_len {
        if let Some(p) = user_periods.get(i) {
            max_user_period = Some(core::cmp::max(max_user_period.unwrap_or(0), p));
            if e.storage().persistent().has(&DataKey::Wrap(user.clone(), p)) {
                live_wraps_found += 1;
            }
        }
    }

    let all_user_periods_live = if scan_len > 0 {
        live_wraps_found == scan_len
    } else {
        true
    };
    InvariantReport {
        wrap_count,
        user_periods_len,
        wrap_periods_len,
        all_user_periods_live,
        latest_period,
        max_user_period,
        live_wraps_found,
        balance: wrap_count as i128,
    }
}
