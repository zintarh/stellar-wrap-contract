use soroban_sdk::{panic_with_error, Address, Env};

use crate::storage_types::DataKey;
use crate::{ContractError};

use super::ttl::TTL_ONE_YEAR;

/// Set the caller's opt-out flag, preventing any future wraps from being
/// minted for them. Only the user themselves can call this.
pub(crate) fn opt_out(e: Env, user: Address) {
    user.require_auth();
    let key = DataKey::OptOut(user);
    e.storage().persistent().set(&key, &true);
    e.storage().persistent().extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Clear the caller's opt-out flag, allowing future wraps to be minted for
/// them again. Only the user themselves can call this.
pub(crate) fn opt_in(e: Env, user: Address) {
    user.require_auth();
    let key = DataKey::OptOut(user);
    e.storage().persistent().remove(&key);
}

/// Returns `true` if the user has opted out of future mints.
pub(crate) fn is_opted_out(e: &Env, user: &Address) -> bool {
    e.storage().persistent().has(&DataKey::OptOut(user.clone()))
}

/// Panics with `ContractError::UserOptedOut` if `user` has opted out of
/// future mints. Imported by the mint and bridge paths so the guard is
/// enforced consistently wherever a wrap is created.
pub(crate) fn require_not_opted_out(e: &Env, user: &Address) {
    if is_opted_out(e, user) {
        panic_with_error!(e, ContractError::UserOptedOut);
    }
}
