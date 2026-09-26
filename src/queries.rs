//! Read-only query helpers for the wrap contract.
//!
//! This module centralises the storage reads used by the contract's public
//! query entry points. It exposes helpers for fetching individual wrap
//! records, paginated and full listings of a user's wraps, aggregate wrap
//! summaries, token-style balance/count queries, and contract metadata such
//! as the transfer-fee configuration, admin address, admin public key, and
//! overall contract health.
//!
//! All functions here are side-effect free: they only read from persistent or
//! instance storage and never mutate contract state.

use soroban_sdk::{Address, Bytes, BytesN, Env, String, Symbol, Vec};

use crate::{ContractHealth, DataKey, InvariantReport, TransferFeeConfig, WrapRecord, WrapSummary};

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

/// Returns an aggregate summary of a user's active wraps across all periods.
///
/// Returns `None` if the user has no active wraps.
///
/// The summary includes:
/// - `total_wraps`: count of active wrap records
/// - `periods`: all period IDs (YYYYMM) for active wraps
/// - `archetypes`: unique archetype symbols across all active wraps
/// - `first_period`: the earliest period with an active wrap
/// - `latest_period`: the latest period with an active wrap
pub(crate) fn get_wrap_summary(e: Env, user: Address) -> Option<WrapSummary> {
    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    let periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)?;

    if periods.is_empty() {
        return None;
    }

    let mut total_wraps: u32 = 0;
    let mut archetypes: Vec<Symbol> = Vec::new(&e);
    let mut first_period: u64 = u64::MAX;
    let mut latest_period: u64 = 0;

    for i in 0..periods.len() {
        if let Some(period) = periods.get(i) {
            if let Some(wrap) = e
                .storage()
                .persistent()
                .get::<_, WrapRecord>(&DataKey::Wrap(user.clone(), period))
            {
                total_wraps += 1;

                if period < first_period {
                    first_period = period;
                }
                if period > latest_period {
                    latest_period = period;
                }

                if !archetypes.contains(&wrap.archetype) {
                    archetypes.push_back(wrap.archetype);
                }
            }
        }
    }

    if total_wraps == 0 {
        return None;
    }

    Some(WrapSummary {
        total_wraps,
        periods,
        archetypes,
        first_period,
        latest_period,
    })
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
/// Derived from `Cargo.toml` package version at compile time via
/// `CARGO_PKG_VERSION`, so t

/* … truncated 3687 chars — edit only what you need near the top … */
