use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::{wrap_record, ContractError, DataKey};

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
///
/// # State mutations (revoke-specific)
/// All record removal is delegated to `wrap_record::remove_wrap_record`.
/// This function additionally increments `TotalRevoked` in instance storage,
/// which is a revocation-only metric and not part of the shared removal path.
pub(crate) fn revoke_wrap(e: Env, user: Address, period: u64, reason_hash: BytesN<32>) {
    // ── Authorization ─────────────────────────────────────────────────────
    let admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    admin.require_auth();

    // ── Shared record removal (identical state delta to burn_wrap) ────────
    wrap_record::remove_wrap_record(&e, &user, period);

    // ── Revoke-specific: increment TotalRevoked in instance storage ───────
    let total_revoked_key = DataKey::TotalRevoked;
    let current_total: u64 = e
        .storage()
        .instance()
        .get(&total_revoked_key)
        .unwrap_or(0);
    e.storage()
        .instance()
        .set(&total_revoked_key, &(current_total + 1));

    // ── Event ─────────────────────────────────────────────────────────────
    e.events()
        .publish((symbol_short!("revoke"), user, period), reason_hash);
}
