//! TTL management for persistent storage entries.
//!
//! Soroban persistent storage entries expire after their TVL lapses. This
//! module owns the renewal helpers so the `lib.rs` facade stays a pure
//! delegation layer.

use soroban_sdk::{panic_with_error, Address, Env, Vec};

use crate::admin;
use crate::mint::MAX_BATCH_SIZE;
use crate::storage_types::DataKey;
use crate::ContractError;

/// TVL (ledgers) applied to persistent entries (/1 year at 5s/ledger).
const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Temporary TTL constant for short-lived data (30 days).
pub(crate) const TTL_TEMP: u32 = 17_280 * 30;

/// Calculate the ledger number when a renewed entry will expire.
///
/// Returns the current ledger sequence + TTL_ONE_YEAR.
fn calculate_live_until_ledger(e: &Env) -> u32 {
    e.ledger().sequence() + TTL_ONE_YEAR
}

/// Extend the TTL (time-to-live) for all persistent storage entries belonging
/// to a user.
///
/// Soroban persistent storage entries expire after their TTL lapses. This
/// function lets anyone renew a user's wrap records so they remain accessible
/// indefinitely.
///
/// # TTL Lifecycle
///
/// All persistent storage entries are stored with a TTL of ~1 year.
///
/// **Automatic renewal (metadata only):** When `mint_wrap` is called, the
/// `WrapCount` and `WrapMetadata` entries are automatically renewed.
///
/// **Manual renewal (individual wraps):** Historical wrap records for specific
/// `(user, token)` pairs are **not** automatically renewed. Anyone can call
/// this `extend_ttl` function to renew a specific wrap record.
///
/// **Bulk renewal (admin):** The `renew_all_ttls` function allows the admin to
/// extend the TTL of all metadata entries for a user. Full wrap-entry renewal
/// requires individual records.
///
/// **Batch renewal (permissionless):** Because a user can have many wrap
/// records, [`extend_ttl_batch`] renews a bounded set of a user's records in a
/// single call so a user can keep all their records alive in one transaction.
///
/// # Parameters
/// - `user`: The address whose storage entries will be extended.
/// - `period`: The specific wrap period whose record TTL will be extended.
pub(crate) fn extend_ttl(e: Env, user: Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);

    // Without a matching record there is nothing to renew. Return before any
    // metadata or instance write so a caller cannot bump the contract instance
    // TTL with a call that has no other effect.
    if !e.storage().persistent().has(&wrap_key) {
        return;
    }

    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    extend_user_metadata_ttl(&e, &user);
    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Renew the per-user metadata keys (`WrapCount`, `WrapMetadata`) when present.
///
/// Shared by [`extend_ttl`] and [`extend_ttl_batch`] so both entrypoints apply
/// the same one-year window to the metadata the user's records depend on.
fn extend_user_metadata_ttl(e: &Env, user: &Address) {
    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let metadata_key = DataKey::WrapMetadata(user.clone());
    if e.storage().persistent().has(&metadata_key) {
        e.storage()
            .persistent()
            .extend_ttl(&metadata_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }
}

/// Extend the TTL of several of a user's wrap records in a single call.
///
/// This is the batch form of [`extend_ttl`], aimed at keeping a user's wrap
/// records alive. One call renews up to [`MAX_BATCH_SIZE`] records.
///
/// # Parameters
/// - `user`: The address whose wrap record TTLs will be extended.
/// - `periods`: The wrap periods to renew, in any order. Periods with no
///   stored record are skipped, so a single stale period does not fail the
///   batch. Duplicates are harmless (renewing an entry twice is idempotent).
///
/// # TTL Lifecycle
///
/// Each matching wrap record is renewed by ~1 year, and the per-user metadata
/// keys plus the contract instance TTL are renewed once per call — not once
/// per period. Like [`extend_ttl`], a batch that matches **no** records is a
/// no-op and does not extend the instance TTL.
///
/// # Authorization
/// Permissionless, exactly like [`extend_ttl`]: no `require_auth`, so a caller
/// needs no special role.
///
/// # Panics
/// - `ContractError::BatchEmpty` if `periods` is empty.
/// - `ContractError::BatchTooLarge` if `periods` holds more than
///   [`MAX_BATCH_SIZE`] entries.
pub(crate) fn extend_ttl_batch(e: Env, user: Address, periods: Vec<u64>) {
    if periods.is_empty() {
        panic_with_error!(&e, ContractError::BatchEmpty);
    }
    if periods.len() > MAX_BATCH_SIZE {
        panic_with_error!(&e, ContractError::BatchTooLarge);
    }

    let mut renewed_any = false;
    for period in periods.iter() {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().persistent().has(&wrap_key) {
            e.storage()
                .persistent()
                .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
            renewed_any = true;
        }
    }

    // Mirror `extend_ttl`: a batch that matched nothing must not write the
    // per-user metadata or the contract instance TTL.
    if !renewed_any {
        return;
    }

    extend_user_metadata_ttl(&e, &user);
    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Admin-only function to extend the TTL for all metadata entries associated
/// with a user.
///
/// This extends the TTL for `WrapCount` and `WrapMetadata` entries. Individual
/// wrap records are **not** extended — use [`extend_ttl`] or
/// [`extend_ttl_batch`] for those.
///
/// # Authorization
/// Requires authorization as the contract admin.
///
/// # Panics
/// - `ContractError::NotInitialized` if the contract has not been initialized.
pub(crate) fn renew_all_ttls(e: Env, user: Address) {
    let admin: Address = admin::read_admin(&e);
    admin.require_auth();

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let metadata_key = DataKey::WrapMetadata(user);
    if e.storage().persistent().has(&metadata_key) {
        e.storage()
            .persistent()
            .extend_ttl(&metadata_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}
