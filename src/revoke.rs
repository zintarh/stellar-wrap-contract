use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::storage_accounting;
use crate::{ContractError, DataKey};

/// Revokes an existing wrap record for the given user and period.
///
/// # Authorization
/// Only the contract admin can revoke wraps. The caller must provide admin
/// authorization via `require_auth()`.
///
/// # reason_hash
/// An optional SHA-256 hash of an off-chain revocation reason. This allows
/// auditors to link a revocation to off-chain evidence (e.g., a governance
/// vote, dispute resolution, or compliance action) without exposing private
/// data on-chain. Pass a zero-filled `BytesN<32>` to omit the reason.
///
/// # Privacy Considerations
/// - The `reason_hash` is a one-way hash. It cannot be reversed to reveal the
///   original reason, preserving confidentiality of sensitive decisions.
/// - Evidence handling: the off-chain reason should be stored in a verifiable
///   location (e.g., a signed document, IPFS, or a governance log) so that
///   auditors can recompute the hash and confirm it matches the on-chain value.
/// - If no reason is provided (all-zero hash), the event still emits for
///   transparency, but without a link to off-chain evidence.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn revoke_wrap(e: Env, user: Address, period: u64, reason_hash: BytesN<32>) {
    let admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    admin.require_auth();

    let wrap_key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapNotFound);
    }

    // Remove the wrap entry and subtract estimated bytes
    e.storage().persistent().remove(&wrap_key);
    storage_accounting::sub_storage_bytes(&e, storage_accounting::estimate_wrap_bytes_new());

    // Decrement the user's wrap count
    let count_key = DataKey::WrapCount(user.clone());
    let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);

    if current_count > 0 {
        let next_count = current_count - 1;
        e.storage().persistent().set(&count_key, &next_count);
        // If count became zero, we consider removing the count entry overhead
        if next_count == 0 {
            storage_accounting::sub_storage_bytes(
                &e,
                storage_accounting::estimate_wrapcount_bytes_new(),
            );
            // Optionally remove the key entirely (keep it set to 0 for now to match existing behavior)
        }
    }

    // Clear latest period if we just revoked it
    let latest_key = DataKey::LatestPeriod(user.clone());
    let current_latest: Option<u64> = e.storage().persistent().get(&latest_key);
    if current_latest == Some(period) {
        e.storage().persistent().remove(&latest_key);
    }

    // Remove period from user's period lists (UserPeriods and WrapPeriods)
    for key in [DataKey::UserPeriods(user.clone()), DataKey::WrapPeriods(user.clone())] {
        if let Some(mut periods) = e.storage().persistent().get::<_, soroban_sdk::Vec<u64>>(&key) {
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
                    e.storage().persistent().set(&key, &periods);
                } else {
                    e.storage().persistent().remove(&key);
                }
            }
        }
    }

    let total_revoked_key = DataKey::TotalRevoked;
    let current_total: u64 = e.storage().temporary().get(&total_revoked_key).unwrap_or(0);
    let next_total = current_total + 1;
    e.storage().instance().set(&total_revoked_key, &next_total);

    // Record the revocation timestamp in the user's last-updated marker.
    crate::mint::update_last_updated(&e, &user);

    e.events()
        .publish((symbol_short!("revoke"), user, period), reason_hash);
}
