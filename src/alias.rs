//! Alias semantics
//!
//! An *alias* is a 32-byte hash that a user may attach to their own address.
//! It is purely a self-declared label: it is not a second account, it does not
//! hold funds, and it is never used as a storage key for balances or wrap
//! records.
//!
//! * Who sets it: only the address itself. `set_alias_hash` calls
//!   `user.require_auth()`, so no admin or third party can write an alias on
//!   behalf of someone else.
//! * Who reads it: `get_alias_hash` is the only reader. `balance_of`,
//!   wrap-record ownership, and the mint guard all key off the raw `Address`
//!   and never consult `DataKey::AliasHash`, so an alias cannot create a
//!   second key into the same records.
//!
//! Because an alias is a label rather than an identity, it must not be usable
//! to shadow another user. Two invariants are enforced here:
//!
//! 1. An alias hash may not equal the raw 32-byte representation of any
//!    address (the caller's own included), which would let a label collide
//!    with a real account key.
//! 2. An alias hash may not already be claimed by a different address, so a
//!    user cannot take over a label that another user has registered.
//!
//! Both violations are rejected with `Error::AliasConflict`.

use soroban_sdk::{Address, BytesN, Env};

use crate::constants::TTL_ONE_YEAR;
use crate::errors::Error;
use crate::DataKey;

/// Store a 32-byte alias hash for the calling user.
///
/// `require_auth` is called so only the user themselves can set or update
/// their alias hash — no admin involvement required.
///
/// Rejects the write with `Error::AliasConflict` if the hash would shadow an
/// address or is already claimed by another user. Re-setting the caller's own
/// current alias is allowed (idempotent update).
pub(crate) fn set_alias_hash(e: Env, user: Address, alias_hash: BytesN<32>) -> Result<(), Error> {
    user.require_auth();

    // An alias must not shadow a real address key. The caller's own address is
    // included: a label equal to an account key is ambiguous and rejected.
    if alias_hash == user.clone().to_bytes() {
        return Err(Error::AliasConflict);
    }

    // An alias must be unique across users. Re-setting the caller's own alias
    // is permitted; claiming a hash already owned by someone else is not.
    let owner_key = DataKey::AliasOwner(alias_hash.clone());
    if let Some(existing) = e.storage().persistent().get::<DataKey, Address>(&owner_key) {
        if existing != user {
            return Err(Error::AliasConflict);
        }
    }

    let key = DataKey::AliasHash(user.clone());
    e.storage().persistent().set(&key, &alias_hash);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    e.storage().persistent().set(&owner_key, &user);
    e.storage()
        .persistent()
        .extend_ttl(&owner_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    Ok(())
}

/// Return the alias hash for `user`, or `None` if not set.
///
/// This is the only reader of alias state; balances, wrap records, and the
/// mint guard are keyed by `Address` and are unaffected by aliases.
pub(crate) fn get_alias_hash(e: Env, user: Address) -> Option<BytesN<32>> {
    e.storage().persistent().get(&DataKey::AliasHash(user))
}
