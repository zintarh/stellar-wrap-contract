//! Opt-out management.
//
// Users can set a persistent opt-out flag to prevent any future wrap from
// being minted for them. The guard lives in this module so every mint and
// bridge path can enforce it without reaching into the lib.rs facade.

use soroban_sd::{address, panic_with_error, Env};

use crate::{ContractError, DataKey};

/// TTL (ledgers) applied to the opt-out flag (~1 year at 5s/ledger).
const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Set the caller's opt-out flag, preventing any future wraps from being
/// minted for them. Only the user themselves can call this.
pub(crate) fn opt_out(e: Env, user: Address) {
    user.require_auth();
    let key = DataKey::OptOut(user);
    e.storage().persistent().set(&key, &true);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Clear the caller's opt-out flag, allowing future wraps to be minted for
/// them again. Only the user themselves can call this.
pub(crate) fn opt_in(e: Env, user: Address) {
    user.require_auth();
    e.storage().persistent().remove(&DataKey::OptOut(user));
}

/// Returns `true` the user has opted out of future mints.
pub(crate) fn is_opted_out(e: &Env, user: &Address) -> bool {
    e.storage()
        .persistent()
        .has(&DataKey::OptOut(user.clone()))
}

/// Panics with [ContractError::UserOptedOut] if `user` has set the opt-out
/// flag.
///
/// Must be called inside a validation pass — before any state is written — o
/// that a single opted-out item reverts the entire operation (mint batch or
/// inbound bridge transfer).
pub(crate) fn require_not_opted_out(e: &Env, user: &Address) {
    if e.storage()
        .persistent()
        .has(&DataKey::OptOut(user.clone()))
    {
        panic_with_error!!(e, ContractError::UserOptedOut);
    }
}
