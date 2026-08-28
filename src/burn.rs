use soroban_sdk::{symbol_short, Address, Env};

use crate::wrap_record;

/// Burns (permanently deletes) a wrap record owned by the caller.
///
/// # Authorization
/// Only the wrap owner can burn their own wrap. The caller must provide
/// authorization via `require_auth()` and the wrap must exist in storage.
///
/// # Arguments
/// * `user` - The address of the wrap owner
/// * `period` - The period (YYYYMM) of the wrap to delete
///
/// # Errors
/// * `WrapNotFound` if the wrap_id (user, period pair) does not exist in storage
/// * `Unauthorized` if caller is not the wrap owner
///
/// # Side Effects
/// All record removal (Wrap key, WrapPeriods, WrapCount, LatestPeriod,
/// UserPeriods backward-compat, LastUpdated, storage accounting) is delegated
/// to `wrap_record::remove_wrap_record`, producing an identical state delta to
/// `revoke_wrap` for the same record.
///
/// # Notes
/// Once burned, the wrap_id is freed and the record cannot be recovered.
/// The user can later mint a new wrap for the same period if desired.
pub(crate) fn burn_wrap(e: Env, user: Address, period: u64) {
    // ── Authorization: owner only ─────────────────────────────────────────
    user.require_auth();

    // ── Shared record removal (identical state delta to revoke_wrap) ──────
    wrap_record::remove_wrap_record(&e, &user, period);

    // ── Event ─────────────────────────────────────────────────────────────
    e.events()
        .publish((symbol_short!("burn"), user.clone(), period), user);
}
