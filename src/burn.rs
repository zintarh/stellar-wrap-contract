use soroban_sdk::{panic_with_error, symbol_short, Address, Env};

use crate::storage_types::{WrapRecord, WrapState};
use crate::{ContractError, DataKey};

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
/// 1. Removes the wrap record from persistent storage (reclaiming its
///    accounted storage bytes)
/// 2. Decrements the user's wrap count (removing key when count reaches zero)
/// 3. Updates WrapPeriods ownership index (removing key when empty)
/// 4. Updates UserPeriods list (removing key when empty)
/// 5. Updates LatestPeriod from remaining WrapPeriods, or removes key when empty
/// 6. Leaves the lifetime TotalWrapCount unchanged, matching `revoke_wrap`
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

    let record: WrapRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Wrap(user.clone(), period))
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));
    // A bridged-out record lives on the destination chain until refunded.
    if record.lifecycle.state == WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    // Same removal and index bookkeeping as `revoke_wrap`.
    crate::revoke::remove_wrap_and_reindex(&e, &user, period);

    crate::mint::update_last_updated(&e, &user);

    // Emit burn event AFTER all state mutations
    e.events()
        .publish((symbol_short!("burn"), user.clone(), period), user);
}
