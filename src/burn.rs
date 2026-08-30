use soroban_sdk::{panic_with_error, symbol_short, Address, Env};

use crate::{ContractError, DataKey, WrapRecord, WrapState};

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
/// 1. Removes the wrap record from persistent storage
/// 2. Decrements the user's wrap count
/// 3. Clears the latest period if the burned wrap was the latest
/// 4. Removes the period from the user's period list
/// 5. Emits a `wrap_burned` event after deletion
///
/// # Notes
/// Once burned, the wrap_id is freed and the record cannot be recovered.
/// The user can later mint a new wrap for the same period if desired.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn burn_wrap(e: Env, user: Address, period: u64) {
    // 1. Require auth FIRST — verify caller is the owner
    user.require_auth();

    // 2. Load wrap — error if not found
    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapNotFound);
    }

    let record: WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();
    if record.fsm.state == WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    // 3. Delete the wrap record from storage
    e.storage().persistent().remove(&wrap_key);

    // 4. Decrement the user's wrap count
    let count_key = DataKey::WrapCount(user.clone());
    let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
    if current_count > 0 {
        e.storage()
            .persistent()
            .set(&count_key, &(current_count - 1));
    }

    // 5. Clear latest period if we just burned it
    let latest_key = DataKey::LatestPeriod(user.clone());
    let current_latest: Option<u64> = e.storage().persistent().get(&latest_key);
    if current_latest == Some(period) {
        e.storage().persistent().remove(&latest_key);
    }

    // 6. Remove period from user's period list
    let user_periods_key = DataKey::UserPeriods(user.clone());
    let mut periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&user_periods_key)
        .unwrap_or(soroban_sdk::Vec::new(&e));

    // Find and remove the period from the list
    let mut found_index: Option<u32> = None;
    for (i, p) in periods.iter().enumerate() {
        if p == period {
            found_index = Some(i as u32);
            break;
        }
    }

    if let Some(idx) = found_index {
        periods.remove(idx);
        if !periods.is_empty() {
            e.storage().persistent().set(&user_periods_key, &periods);
        } else {
            // If no periods remain, remove the key entirely
            e.storage().persistent().remove(&user_periods_key);
        }
    }

    // 7. Emit burn event AFTER state mutation
    e.events()
        .publish((symbol_short!("burn"), user.clone(), period), user);
}
