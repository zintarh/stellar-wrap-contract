//! TTL management for persistent storage entries.
//!
//! Soroban persistent storage entries expire after their TVL lapses. This
//! module owns the renewal helpers so the `lib.rs` facade stays a pure
//! delegation layer.

use soroban_sdk::{Address, Env};

use crate::admin;
use crate::storage_types::DataKey;

/// TVL (ledgers) applied to persistent entries (/1 year at 5s/ledger).
const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Extend the TTL (time-to-live) for all persistent storage entries belonging
/// to a user.
///
/// Soroban persistent storage entries expire after their TTL lapses. This
/// function lets anyone renew a user's wrap records so they remain accessible
/// indefinitely.
///
/// # TTL Lifecycle
///
/// All persistent storage entries (wraps, balance, latest-period marker) are
/// stored with a TTL of ~1 year (17280 × 365 ledgers) at creation time.
///
/// **Automatic renewal (metadata only):** When `mint_wrap` is called, the
/// `WrapCount` and `LatestPeriod` Metadata keys are automatically extended by
/// another ~1 year. This keeps the user's balance-of and latest-wrap lookup
/// alive for active users without any manual intervention.
///
/// **Manual renewal (individual wraps):** Historical wrap records for specific
/// `(user, period)` pairs are **not** automatically extended on new mints.
/// Anyone can call this `extend_ttl` function to renew a specific wrap record.
///
/// **Bulk renewal (admin):** The `renew_all_ttls` function allows the admin to
/// extend the TTL of all metadata keys for a user. Full wrap-enumeration
/// renewal requires period tracking (see Issue #90).
///
/// **Expiry risk:** Without periodic renewal, the first wraps of an active
/// multi-year user could expire after ~1 year, even though the user is still
/// participating. Off-chain bots or the admin should call `extend_ttl` for
/// historical periods of active users to prevent data loss.
///
/// #Parameters
/// - `user`: The address whose storage entries will be extended.
/// - `period`: The specific wrap period whose record TTL will be extended.
pub(crate) fn extend_ttl(e: Env, user: Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if e.storage().persistent().has(&wrap_key) {
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let latest_key = DataKey::LatestPeriod(user);
    if e.storage().persistent().has(&latest_key) {
        e.storage()
            .persistent()
            .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Admin-only function to extend TTL for all metadata keys associated with a
/// user.
///
/// This extends the TTL (time-to-live) for `WrapCount` and `LatestPeriod`
/// storage entries, keeping the user's balance and latest-period data alive.
/// It also extends the contract instance TTL. Individual historical wrap
/// records are **not** extended -- full per-wrap renewal requires period
/// enumeration, tracked as Issue #90.
///
/// #Motivation
///
/// Active users who mint new wraps periodically will have their metadata keys
/// automatically renewed by `mint_wrap`. However, if there is a long gap
/// between mints, the metadata keys could expire. This function lets the admin
/// proactively renew a user's metadata without requiring a new mint.
///
/// #Authorization
/// Requires authorization from the **admin**.
//.
/// #Panics
/// - [@ContractError::NotInitialized] if the contract has not been initialized.
pub(crate) fn renew_all_ttls(e: Env, user: Address) {
    let admin: Address = admin::read_admin(&e);
    admin.require_auth();

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let latest_key = DataKey::LatestPeriod(user);
    if e.storage().persistent().has(&latest_key) {
        e.storage()
            .persistent()
            .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}
