use soroban_sdk::{Address, BytesN, Env};

use crate::constants::TTL_ONE_YEAR;
use crate::DataKey;

/// Store a 32-byte alias hash for the calling user.
///
/// `require_auth` is called so only the user themselves can set or update
/// their alias hash — no admin involvement required.
pub(crate) fn set_alias_hash(e: Env, user: Address, alias_hash: BytesN<32>) {
    user.require_auth();
    let key = DataKey::AliasHash(user);
    e.storage().persistent().set(&key, &alias_hash);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Return the alias hash for `user`, or `None` if not set.
pub(crate) fn get_alias_hash(e: Env, user: Address) -> Option<BytesN<32>> {
    e.storage().persistent().get(&DataKey::AliasHash(user))
}
