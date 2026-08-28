use crate::{ContractHealth, DataKey, InvariantReport, TransferFeeConfig, WrapRecord};
use soroban_sdk::{Address, Bytes, BytesN, Env, String};

pub const MAX_QUERY_RESULTS: u32 = 100;

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

pub(crate) fn balance_of(e: Env, user: Address) -> i128 {
    let count_key = DataKey::WrapCount(user);
    e.storage()
        .persistent()
        .get::<_, u32>(&count_key)
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

    // A revoked latest record clears the marker but leaves older periods intact.
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

pub(crate) fn get_all_wraps_for_user(e: Env, user: Address) -> soroban_sdk::Vec<WrapRecord> {
    // Fetch all wraps by using the maximum possible range.
    get_wraps(e, user, 0, u32::MAX)
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
        .temporary()
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

pub(crate) fn decimals(_e: Env) -> u32 {
    0
}

pub(crate) fn contract_version(e: Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::ContractVersion)
        .unwrap_or(0)
}

pub(crate) fn check_user_invariants(e: Env, user: Address) -> InvariantReport {
    let wrap_count_key = DataKey::WrapCount(user.clone());
    let observed_wrap_count = e.storage().persistent().get::<_, u32>(&wrap_count_key).unwrap_or(0);

    let user_periods_key = DataKey::UserPeriods(user.clone());
    let user_periods = e.storage().persistent().get::<_, soroban_sdk::Vec<u64>>(&user_periods_key);
    let observed_user_periods_len = user_periods.as_ref().map(|p| p.len()).unwrap_or(0);

    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    let wrap_periods = e.storage().persistent().get::<_, soroban_sdk::Vec<u64>>(&wrap_periods_key);
    let observed_wrap_periods_len = wrap_periods.as_ref().map(|p| p.len()).unwrap_or(0);

    let latest_period_key = DataKey::LatestPeriod(user.clone());
    let observed_latest_period = e.storage().persistent().get::<_, u64>(&latest_period_key);

    let observed_max_user_period = user_periods.as_ref().and_then(|p| {
        let mut max = None;
        for i in 0..p.len() {
            if let Some(period) = p.get(i) {
                if max.is_none() || period > max.unwrap() {
                    max = Some(period);
                }
            }
        }
        max
    });

    let observed_balance = balance_of(e.clone(), user.clone());

    let mut user_periods_all_live = true;
    if let Some(p) = &user_periods {
        let len = core::cmp::min(p.len(), MAX_QUERY_RESULTS);
        for i in 0..len {
            if let Some(period) = p.get(i) {
                if !has_wrap(e.clone(), user.clone(), period) {
                    user_periods_all_live = false;
                    break;
                }
            }
        }
    }

    InvariantReport {
        count_matches_user_periods: observed_wrap_count == observed_user_periods_len,
        count_matches_wrap_periods: observed_wrap_periods_len == 0 || observed_wrap_count == observed_wrap_periods_len,
        latest_is_max_user_period: observed_latest_period == observed_max_user_period,
        user_periods_all_live,
        balance_matches_wrap_count: observed_balance == (observed_wrap_count as i128),
        
        observed_wrap_count,
        observed_user_periods_len,
        observed_wrap_periods_len,
        observed_latest_period: observed_latest_period.unwrap_or(0),
        observed_max_user_period: observed_max_user_period.unwrap_or(0),
        observed_balance,
    }
}
