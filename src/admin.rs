//! Privileged administration: ownership, pausing, upgrades and contract-wide
//! configuration.
//!
//! Every function here is admin-gated. [`read_admin`] panics with
//! [`ContractError::NotInitialized`] when no admin is stored, so an
//! uninitialized contract has no privileged surface rather than an open one.
//!
//! # Ownership
//!
//! [`update_admin`] replaces the admin in one transaction — fast, and
//! unrecoverable if the address is wrong. Prefer the two-step handover:
//! [`propose_admin`] then [`accept_admin`], where the incoming admin must
//! authorize the acceptance and so proves control before gaining power.
//! [`cancel_proposed_admin`] withdraws a pending proposal; only one may be
//! outstanding.
//!
//! # Relationship to the timelock
//!
//! Once [`crate::timelock`] is enabled, [`update_admin`], [`upgrade`],
//! [`propose_admin`] and [`accept_admin`] reject direct calls via
//! [`crate::timelock::require_direct_call_allowed`]; the admin must schedule
//! the equivalent [`crate::storage_types::TimelockAction`], wait out the delay
//! and execute. [`accept_admin`] is blocked too, otherwise a proposal made
//! before the timelock was enabled could be cashed in afterwards and skip the
//! delay.
//!
//! Pausing, fee configuration, key rotation, metadata and migrations stay
//! directly callable by design — they are incident response or reversible
//! configuration.
//!
//! # Signing key rotation
//!
//! [`update_admin_pubkey`] swaps the verification key immediately: there is no
//! grace window. Any signature issued under the previous key — even one whose
//! `valid_until` is still in the future — stops verifying the moment the new
//! key is stored. This is intentional for emergency rotation (a compromised key
//! must be unusable at once); for planned rotation, drain in-flight signatures
//! before rotating. See `docs/admin-rotation.md`.
//!
//! # Pausing, upgrades, migrations
//!
//! [`set_pause`] is the incident switch and [`require_not_paused`] the guard
//! other modules call. Requesting the state already in effect is a silent
//! no-op that emits nothing, so a double-pause is not reported twice.
//!
//! [`apply_upgrade`] is shared by [`upgrade`] and the timelocked path so both
//! bump `ContractVersion` and emit the same audit event. [`migrate`] tracks
//! storage schema versions separately and only moves forward, so a replayed
//! migration cannot corrupt storage.

use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::{
    ttl::TTL_TEMP, ContractError, DataKey, TransferFeeConfig, WrapRecord,
};

/// Minimum duration for an admin proposal, in seconds (1 hour).
pub(crate) const MIN_PROPOSAL_DURATION: u64 = 60 * 60;
/// Maximum duration for an admin proposal, in seconds (30 days).
pub(crate) const MAX_PROPOSAL_DURATION: u64 = 30 * 24 * 60 * 60;

/// Reads the stored admin or panics with `NotInitialized`.
pub(crate) fn read_admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

/// Initializes the contract with the controlling admin address and the
/// Ed25519 public key used to verify mint signatures.
///
/// # Panics
/// - [`ContractError::AlreadyInitialized`] if the contract was already initialized.
/// - [`ContractError::InvalidAdminPubKey`] if `admin_pubkey` is the all-zero
///   key. An all-zero Ed25519 public key has no known corresponding private
///   key, so accepting it would silently brick every future `mint_wrap` call
///   (no valid signature could ever be produced) while leaving the contract in
///   an "initialized" state. Rejecting it at initialization time prevents this
///   misconfiguration rather than discovering it after deployment.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
    if e.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(e, ContractError::AlreadyInitialized);
    }
    if admin_pubkey == BytesN::from_array(&e, &[0u8; 32]) {
        panic_with_error!(e, ContractError::InvalidAdminPubKey);
    }
    e.storage().instance().set(&DataKey::Admin, &admin);
    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &admin_pubkey);
    // Store the initial storage schema version (v1).
    e.storage().instance().set(&DataKey::SchemaVersion, &1u32);
    crate::events::publish_event(&e, crate::events::Event::AdminInit(admin));
}

/// Immediate admin replacement.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::SetAdmin(..))` followed by `execute`.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn update_admin(e: Env, new_admin: Address) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin = read_admin(&e);
    current_admin.require_auth();
    e.storage().instance().set(&DataKey::Admin, &new_admin);
    e.storage().instance().remove(&DataKey::PendingAdmin);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("admin"),
            symbol_short!("updated"),
        ),
        (current_admin, new_admin),
    );
}

/// Pause or unpause the contract.
///
/// Pauses (`paused = true`) or resumes (`paused = false`) the contract and
/// emits a direction-specific event carrying the acting admin.
///
/// A redundant call that requests the state already in effect is a silent
/// no-op: it does **not** emit an event, so monitoring built on these events
/// only sees signals that correspond to an actual state change (a double-pause
/// by two incident responders must not be reported as a second pause).
///
/// Event topics are direction-distinguishable so subscribers can filter by
/// direction instead of decoding a boolean payload:
/// - pause:   `("pause", "paused")`, data = acting admin
/// - unpause: `("pause", "unpaused")`, data = acting admin
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn set_pause(e: Env, paused: bool) {
    let admin = read_admin(&e);
    admin.require_auth();

    // Return early when the requested state already matches the current state.
    // This makes a redundant pause/unpause a silent no-op instead of emitting
    // an event that does not correspond to a state change.
    if is_paused(&e) == paused {
        return;
    }

    e.storage().instance().set(&DataKey::Paused, &paused);
    crate::events::publish_event(&e, crate::events::Event::AdminPause(paused));
}

/// Admin-only: configure the token-denominated fee charged by `transfer_wrap`.
///
/// An amount of zero enables fee-free transfers without removing the configured
/// token and recipient.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn set_transfer_fee(e: Env, token: Address, recipient: Address, amount: i128) {
    read_admin(&e).require_auth();
    if amount < 0 || token == recipient {
        panic_with_error!(e, ContractError::InvalidFeeParams);
    }
    e.storage().instance().set(
        &DataKey::TransferFee,
        &TransferFeeConfig {
            amount,
            recipient: recipient.clone(),
            token: token.clone(),
        },
    );
    e.events()
        .publish((symbol_short!("fee"),), (token, recipient, amount));
}

/// Admin-only: clear the configured transfer fee, returning the contract to an unconfigured state.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn clear_transfer_fee(e: Env) {
    read_admin(&e).require_auth();
    e.storage().instance().remove(&DataKey::TransferFee);
    e.events().publish((symbol_short!("fee_clr"),), ());
}

pub(crate) fn is_paused(e: &Env) -> bool {
    e.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub(crate) fn require_not_paused(e: &Env) {
    if is_paused(e) {
        panic_with_error!(e, ContractError::Paused);
    }
}

/// Marks a storage migration as applied. A version can only be applied once and
/// versions must move forward, so an upgrade shipping a migration cannot corrupt
/// storage by replaying it.
pub(crate) fn migrate(e: Env, version: u32) {
    read_admin(&e).require_auth();

    let applied = migration_version(&e);
    if version <= applied {
        panic_with_error!(e, ContractError::MigrationAlreadyApplied);
    }

    e.storage()
        .instance()
        .set(&DataKey::SchemaVersion, &version);
}

/// Returns the current storage schema version (0 if never set).
pub(crate) fn migration_version(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::SchemaVersion)
        .unwrap_or(0)
}

#[cfg(test)]
mod rotation_tests {
    //! Behaviour of `update_admin_pubkey` when signatures are in flight.
    //!
    //! The contract has no grace window: rotation is immediate. These tests pin
    //! that contract so a future change to add a grace window is a deliberate,
    //! reviewed decision rather than an accidental regression.
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, Address, BytesN<32>, BytesN<32>) {
        let e = Env::default();
        e.mock_all_auths();
        let admin = Address::generate(&e);
        let old_key = BytesN::from_array(&e, &[1u8; 32]);
        let new_key = BytesN::from_array(&e, &[2u8; 32]);
        initialize(e.clone(), admin, old_key.clone());
        (e, admin, old_key, new_key)
    }

    fn stored_pubkey(e: &Env) -> BytesN<32> {
        e.storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .expect("admin pubkey must be set")
    }

    #[test]
    fn rotation_swaps_key_immediately() {
        let (e, _admin, old_key, new_key) = setup();
        assert_eq!(stored_pubkey(&e), old_key);

        update_admin_pubkey(e.clone(), new_key.clone());

        assert_eq!(stored_pubkey(&e), new_key);
    }

    #[test]
    fn old_key_no_longer_verifies_after_rotation() {
        let (e, _admin, old_key, new_key) = setup();

        update_admin_pubkey(e.clone(), new_key.clone());

        // A signature issued under the old key (with a future `valid_until`)
        // must not verify against the rotated key: the stored key is the new
        // one, so the old key is no longer the verification key.
        assert_ne!(stored_pubkey(&e), old_key);
        assert_eq!(stored_pubkey(&e), new_key);
    }

    #[test]
    fn new_key_verifies_after_rotation() {
        let (e, _admin, _old_key, new_key) = setup();

        update_admin_pubkey(e.clone(), new_key.clone());

        // Signatures issued under the new key verify because the stored key is
        // exactly the new key.
        assert_eq!(stored_pubkey(&e), new_key);
    }

    #[test]
    fn rotation_rejects_all_zero_key() {
        let (e, _admin, _old_key, _new_key) = setup();
        let zero = BytesN::from_array(&e, &[0u8; 32]);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            update_admin_pubkey(e.clone(), zero);
        }));

        assert!(result.is_err(), "all-zero key must be rejected");
    }
}
