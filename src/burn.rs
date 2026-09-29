use soroban_sdk::{symbol_short, Address, Env};

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
/// * `InvalidStateTransition` if the wrap is in the Bridged state
///
/// # Side Effects
/// 1. Removes the wrap record from persistent storage
/// 2. Decrements the user's wrap count (removing key when count reaches zero)
/// 3. Updates WrapPeriods ownership index (removing key when empty)
/// 4. Updates UserPeriods list (removing key when empty)
/// 5. Updates LatestPeriod from remaining WrapPeriods, or removes key when empty
/// 6. Decrements the global TotalWrapCount
/// 7. Records the state-change timestamp via update_last_updated
/// 8. Emits a `burn` event after deletion
///
/// # Notes
/// Once burned, the wrap_id is freed and the record cannot be recovered.
/// The user can later mint a new wrap for the same period if desired.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn burn_wrap(e: Env, user: Address, period: u64) {
    // Require auth FIRST — verify caller is the owner.
    user.require_auth();

    // Remove the wrap record and unwind all per-user bookkeeping.
    // The helper also enforces WrapNotFound and the Bridged-state guard.
    crate::wrap_record_helpers::remove_wrap_record(&e, &user, period);

    // Emit burn event AFTER state mutation.
    e.events()
        .publish((symbol_short!("burn"), user.clone(), period), user);
}
