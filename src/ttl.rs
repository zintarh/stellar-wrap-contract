use soroban_sdk::Address;

use crate::admin;
use crate::storage_types::DataKey;

/// TTL (in ledgers) applied to persistent wrap/metadata entries (~1 year at
/// 5s/ledger). Shared by the TTL management entrypoints and opt-out storage.
pub(crate) const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Extend the TTL (time-to-live) for all persistent storage entries belonging
/// to a user: the wrap record, the wrap-count and latest-period metadata keys,
/// and the contract instance.
pub(crate) fn extend_ttl(e: soroban_sdk::Env, user: Address, period: u64) {
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if e.storage().persistent().has(&wrap_key) {
        e.storage().persistent().extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage().persistent().extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let latest_key = DataKey::LatestPeriod(user);
    if e.storage().persistent().has(&latest_key) {
        e.storage().persistent().extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Admin-only bulk renewal of a user's metadata TTLs. Uses the shared
/// `admin::read_admin` helper rather than reading storage directly.
pub(crate) fn renew_all_ttls(e: soroban_sdk::Env, user: Address) {
    let admin: Address = admin::read_admin(&e);
    admin.require_auth();

    let count_key = DataKey::WrapCount(user.clone());
    if e.storage().persistent().has(&count_key) {
        e.storage().persistent().extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let latest_key = DataKey::LatestPeriod(user);
    if e.storage().persistent().has(&latest_key) {
        e.storage().persistent().extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    e.storage().instance().extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}
