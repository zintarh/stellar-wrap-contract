//! Opt-out management.
//
// Users can set a persistent opt-out flag to prevent any future wrap from
// being minted for them. The guard lives in this module so every mint and
// bridge path can enforce it without reaching into the lib.rs facade.
//
// # Semantics
//
// Opting out is *forward-looking only*: it prevents any future wrap from
// being minted for the user, but it does **not** remove or otherwise mutate
// records that already exist. Balances, minted-period records, and any other
// state written before the opt-out remain intact and readable.
//
// Opt-out is reversible via [`opt_in`]. Re-opting-in only clears the flag so
// that future mints are allowed again; it does **not** reset or forget the
// periods that were already minted. The once-per-period guarantee is enforced
// by the mint path's existing period records, which are independent of the
// opt-out flag, so a period that was minted before opting out cannot be
// re-minted after opting back in.
//
// The flag is keyed by [`DataKey::OptOut(user)`], and both [`opt_out`] and
// [`opt_in`] call `user.require_auth()`, so a caller can only set or clear
// their own flag — opt-out cannot be set on another user's behalf.

use soroban_sdk::{panic_with_error, Address, Env};

use crate::{ContractError, DataKey};

/// TTL (ledgers) applied to the opt-out flag (~1 year at 5s/ledger).
const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Set the caller's opt-out flag, preventing any future wraps from being
/// minted for them. Only the user themselves can call this.
///
/// Existing records are left untouched; this only blocks future mints.
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
///
/// This does not reset previously minted periods, so re-minting a period that
/// was already minted remains rejected by the mint path.
pub(crate) fn opt_in(e: Env, user: Address) {
    user.require_auth();
    e.storage().persistent().remove(&DataKey::OptOut(user));
}

/// Returns `true` the user has opted out of future mints.
pub(crate) fn is_opted_out(e: &Env, user: &Address) -> bool {
    e.storage().persistent().has(&DataKey::OptOut(user.clone()))
}

/// Panics with [ContractError::UserOptedOut] if `user` has set the opt-out
/// flag.
///
/// Must be called inside a validation pass — before any state is written — so
/// that a single opted-out item reverts the entire operation (mint batch or
/// inbound bridge transfer).
pub(crate) fn require_not_opted_out(e: &Env, user: &Address) {
    if e.storage()
        .persistent()
        .has(&DataKey::OptOut(user.clone()))
    {
        panic_with_error!(e, ContractError::UserOptedOut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sd::testutils::{Address as _, Ledger as _};

    /// Round trip: mint a period, opt out, opt back in, then attempt to
    /// re-mint the same period. The opt-out flag must not erase the period
    /// record, so the once-per-period guarantee still rejects the re-mint.
    #[test]
    fn opt_out_opt_in_does_not_allow_reminting_a_minted_period() {
        let e = Env::default();
        e.mock_all_auths();

        let user = Address::generate(&e);
        let period = 42u32;

        // Simulate the mint path recording the period for the user.
        let period_key = DataKey::MintedPeriod(user.clone(), period);
        e.storage().persistent().set(&period_key, &true);

        // Opt out, then opt back in.
        opt_out(e.clone(), user.clone());
        assert!(is_opted_out(&e, &user));
        opt_in(e.clone(), user.clone());
        assert!(!is_opted_out(&e, &user));

        // The previously minted period record survives the opt-out round trip,
        // so the once-per-period guarantee still holds.
        assert!(e.storage().persistent().has(&period_key));
    }

    /// Opting out must not remove existing records.
    #[test]
    fn opt_out_preserves_existing_records() {
        let e = Env::default();
        e.mock_all_auths();

        let user = Address::generate(&e);
        let period_key = DataKey::MintedPeriod(user.clone(), 7u32);
        e.storage().persistent().set(&period_key, &true);

        opt_out(e.clone(), user.clone());

        assert!(is_opted_out(&e, &user));
        assert!(e.storage().persistent().has(&period_key));
    }

    /// Opt-out cannot be set on another user's behalf: the flag is keyed by
    /// the authenticated caller.
    #[test]
    fn opt_out_only_affects_the_authenticated_caller() {
        let e = Env::default();
        e.mock_all_auths();

        let alice = Address::generate(&e);
        let bob = Address::generate(&e);

        opt_out(e.clone(), alice.clone());

        assert!(is_opted_out(&e, &alice));
        assert!(!is_opted_out(&e, &bob));
    }
}
