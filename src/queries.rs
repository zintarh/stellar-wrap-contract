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

/// Maximum number of wrap records a single paginated query will read.
///
/// [`get_wraps`] clamps the caller-supplied `limit` to this value so that no
/// single call can read more than `MAX_PAGE_SIZE` records, regardless of what
/// the client requests. Clients should page through results using the
/// `next_start` value returned by the contract's paginated query entry point.
pub(crate) const MAX_PAGE_SIZE: u32 = 50;

/// Documented upper bound for the unpaginated [`get_all_wraps_for_user`]
/// helper. Users with more than this many wraps must page via [`get_wraps`].
pub(crate) const MAX_QUERY_RESULTS: u32 = 200;

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

/// Verifies that `data` matches the SHA-256 hash committed for `(user, period)`.
///
/// Looks up `DataKey::Wrap(user, period)` in persistent storage, computes
/// `SHA-256(data)`, and compares the result to [`WrapRecord::data_hash`].
/// Returns `true` only when the record exists **and** the hashes are equal.
/// Returns `false` for a missing record, a tampered payload, or any mismatch.
///
/// This function is pure read — it never writes storage and requires no auth.
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

/// Returns a page of wrap records owned by `user`.
///
/// Reads at most [`MAX_PAGE_SIZE`] records per call. The caller-supplied
/// `limit` is clamped to `MAX_PAGE_SIZE`, so a request for `limit = 10_000`
/// still reads no more than `MAX_PAGE_SIZE` records. A `limit` of `0` yields
/// an empty result, and a `start` at or beyond the number of stored periods
/// also yields an empty result.
///
/// To continue paging, callers should advance `start` by the number of records
/// returned (or by the effective page size) until an empty page is returned.
pub(crate) fn get_wraps(
    e: Env,
    user: Address,
    start: u32,
    limit: u32,
) -> soroban_sdk::Vec<WrapRecord> {
    let mut results = soroban_sdk::Vec::new(&e);
    let user_periods_key = DataKey::UserPeriods(user.clone());

    // Enforce the server-side maximum page size regardless of client input.
    let effective_limit = core::cmp::min(limit, MAX_PAGE_SIZE);

    if effective_limit == 0 {
        return results;
    }

    if let Some(periods) = e
        .storage()
        .persistent()
        .get::<_, soroban_sdk::Vec<u64>>(&user_periods_key)
    {
        let len = periods.len();
        if start < len {
            let end = core::cmp::min(start.saturating_add(effective_limit), len);
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
/// This is a convenience wrapper around [`get_wraps`] that pages internally
/// through the user's records, reading at most [`MAX_PAGE_SIZE`] records per
/// underlying call. It is intended for bounded queries of at most
/// [`MAX_QUERY_RESULTS`] (200) records; callers with larger datasets should use
/// the paginated [`get_wraps`] directly to stay within Soroban resource limits.
///
/// # Panics
///
/// Panics with `"too many wraps: use paginated get_wraps"` if the user owns
/// more than [`MAX_QUERY_RESULTS`] wraps, so the bound is enforced rather than
/// silently truncated.
pub(crate) fn get_all_wraps_for_user(e: Env, user: Address) -> soroban_sdk::Vec<WrapRecord> {
    let mut all = soroban_sdk::Vec::new(&e);
    let mut start: u32 = 0;

    loop {
        let page = get_wraps(e.clone(), user.clone(), start, MAX_PAGE_SIZE);
        let page_len = page.len();
        if page_len == 0 {
            break;
        }

        for i in 0..page_len {
            if let Some(wrap) = page.get(i) {
                all.push_back(wrap);
            }
        }

        if all.len() > MAX_QUERY_RESULTS {
            panic!("too many wraps: use paginated get_wraps");
        }

        start = start.saturating_add(page_len);
    }

    all
}

/// Returns an aggregate summary of the user's active wraps, or `None` when the
/// user has no active wraps.
///
/// Iterates the user's `WrapPeriods` list and only counts periods whose
/// `Wrap` record is still present in storage. Periods whose record has been
/// expired or archived are skipped. If no records remain after filtering, this
/// returns `None` rather than a summary with `total_wraps == 0`, so callers
/// never observe a nonsensical `first_period`.
pub(crate) fn get_wrap_summary(e: Env, user: Address) -> Option<WrapSummary> {
    let periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::WrapPeriods(user.clone()))?;

    let mut total_wraps: u32 = 0;
    let mut first_period: u64 = u64::MAX;
    let mut latest_period: u64 = 0;

    for index in 0..periods.len() {
        if let Some(period) = periods.get(index) {
            if e.storage()
                .persistent()
                .get::<_, WrapRecord>(&DataKey::Wrap(user.clone(), period))
                .is_some()
            {
                total_wraps = total_wraps.saturating_add(1);
                if period < first_period {
                    first_period = period;
                }
                if period > latest_period {
                    latest_period = period;
                }
            }
        }
    }

    if total_wraps == 0 {
        return None;
    }

    Some(WrapSummary {
        total_wraps,
        first_period,
        latest_period,
    })
}
