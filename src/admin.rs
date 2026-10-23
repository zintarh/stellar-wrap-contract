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
//! # Role-based access control
//!
//! Privileged entrypoints are gated by the narrowest [`Role`] they need rather
//! than by the single admin key. [`grant_role`] and [`revoke_role`] are
//! admin-only; [`renounce_role`] lets a holder drop its own role, except that
//! the last `Admin` cannot renounce itself (which would brick the contract).
//! [`has_role`] is the read-only membership check. The role-to-entrypoint
//! matrix is documented in `docs/authority-model.md`.
//!
//! # Relationship to the timelock
//!
//! Once [`crate::timelock`] is enabled, [`update_admin`], [`upgrade`],
//! [`propose_admin`], [`accept_admin`], [`update_admin_pubkey`],
//! [`propose_signing_key`] and [`accept_signing_key`] reject direct calls via
//! [`crate::timelock::require_direct_call_allowed`]; the admin must schedule
//! the equivalent [`crate::storage_types::TimelockAction`], wait out the delay
//! and execute. [`accept_admin`] and [`accept_signing_key`] are blocked too,
//! otherwise a proposal made before the timelock was enabled could be cashed in
//! afterwards and skip the delay.
//!
//! Pausing, fee configuration, metadata and migrations stay
//! directly callable by design — they are incident response or reversible
//! configuration.
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
//!
//! # Bounded v1 → v2 storage migration
//!
//! [`migrate_users`] rebuilds the v2 `UserIndex` for a bounded batch of users
//! and deletes their legacy v1 keys. It is resumable: [`MigrationState`] records
//! the schema version and how many users have been migrated so far, so the
//! off-chain driver (`scripts/migrate-v2.ts`) can page through the indexer and
//! submit batches until the cursor reaches the total. Re-running a batch is a
//! no-op because already-migrated users are skipped. Unmigrated users are
//! blocked from minting (see `mint::require_migrated`) rather than lazily
//! migrated, so a mint can never exceed the per-transaction budget.

use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, BytesN, Env, Vec};

use crate::{
    ttl::TTL_TEMP, ContractError, DataKey, TransferFeeConfig, WrapRecord,
};

/// Minimum duration for an admin proposal, in seconds (1 hour).
pub(crate) const MIN_PROPOSAL_DURATION: u64 = 60 * 60;
/// Maximum duration for an admin proposal, in seconds (30 days).
pub(crate) const MAX_PROPOSAL_DURATION: u64 = 30 * 24 * 60 * 60;

/// Maximum number of users a single [`migrate_users`] call may process.
///
/// Chosen so one batch stays under Soroban CPU and read/write limits; the
/// bound is asserted in tests. Larger batches must be split by the driver.
pub(crate) const MAX_MIGRATION_BATCH: u32 = 20;

/// A privileged capability in the registry.
///
/// Each privileged entrypoint requires exactly one role; see
/// `docs/authority-model.md` for the role-to-entrypoint matrix.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Full control: grants and revokes every other role.
    Admin,
    /// May mint new wrapped assets.
    Minter,
    /// May revoke wrapped assets.
    Revoker,
    /// May pause and unpause the contract.
    Pauser,
    /// May upgrade the contract WASM.
    Upgrader,
    /// May act on behalf of the bridge (relayed operations).
    Bridge,
}

/// Resumable cursor for the bounded v1 → v2 storage migration.
///
/// `version` is the schema version the migration targets (2 for v2).
/// `users_done` counts users migrated so far and is the resume cursor the
/// off-chain driver reads back to page through the indexer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationState {
    pub version: u32,
    pub users_done: u32,
}

/// Reads the stored admin or panics with `NotInitialized`.
pub(crate) fn read_admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

/// Returns `true` when `account` currently holds `role`.
///
/// The `Admin` role is implicitly held by the stored admin address, so the
/// contract is never left without an admin even before any explicit grant.
pub(crate) fn has_role(e: &Env, role: Role, account: &Address) -> bool {
    if role == Role::Admin && *account == read_admin(e) {
        return true;
    }
    e.storage()
        .persistent()
        .get(&DataKey::RoleMember(role, account.clone()))
        .unwrap_or(false)
}

/// Requires that `account` holds `role`, authorizing the call.
///
/// Panics with [`ContractError::Unauthorized`] when the role is missing.
pub(crate) fn require_role(e: &Env, role: Role, account: &Address) {
    if !has_role(e, role, account) {
        panic_with_error!(e, ContractError::Unauthorized);
    }
    account.require_auth();
}

/// Admin-only: grant `role` to `account`.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn grant_role(e: Env, role: Role, account: Address) {
    let admin = read_admin(&e);
    admin.require_auth();
    e.storage()
        .persistent()
        .set(&DataKey::RoleMember(role, account.clone()), &true);
    e.events().publish(
        (symbol_short!("role"), symbol_short!("granted")),
        (role, account),
    );
}

/// Admin-only: revoke `role` from `account`.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn revoke_role(e: Env, role: Role, account: Address) {
    let admin = read_admin(&e);
    admin.require_auth();
    e.storage()
        .persistent()
        .remove(&DataKey::RoleMember(role, account.clone()));
    e.events().publish(
        (symbol_short!("role"), symbol_short!("revoked")),
        (role, account),
    );
}

/// Lets `account` drop its own `role`.
///
/// The last `Admin` cannot renounce itself: doing so would leave the contract
/// with no one able to grant roles or upgrade it. Panics with
/// [`ContractError::Unauthorized`] in that case.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn renounce_role(e: Env, role: Role, account: Address) {
    account.require_auth();
    if role == Role::Admin && account == read_admin(&e) {
        // The stored admin is the last admin; renouncing would brick the
        // contract. Refuse rather than silently no-op.
        panic_with_error!(e, ContractError::Unauthorized);
    }
    e.storage()
        .persistent()
        .remove(&DataKey::RoleMember(role, account.clone()));
    e.events().publish(
        (symbol_short!("role"), symbol_short!("revoked")),
        (role, account),
    );
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

/// Reads the current migration cursor, defaulting to a fresh v1 state.
pub(crate) fn read_migration_state(e: &Env) -> MigrationState {
    e.storage()
        .instance()
        .get(&DataKey::MigrationState)
        .unwrap_or(MigrationState {
            version: 1,
            users_done: 0,
        })
}

/// Returns `true` when `user` has already been migrated to the v2 layout.
///
/// The v2 `UserIndex` entry is the marker: its presence means the user's
/// legacy keys were already rebuilt and deleted, so a replayed batch skips it.
pub(crate) fn is_user_migrated(e: &Env, user: &Address) -> bool {
    e.storage()
        .persistent()
        .has(&DataKey::UserIndex(user.clone()))
}

/// Upgrader-only: rebuild the v2 `UserIndex` for a bounded batch of users and
/// delete their legacy v1 keys.
///
/// # Bounds
/// At most [`MAX_MIGRATION_BATCH`] users per call so a single invocation stays
/// under Soroban CPU and read/write limits. Panics with
/// [`ContractError::InvalidAmount`] when the batch is empty or too large.
///
/// # Idempotency
/// Users already present in `UserIndex` are skipped, so re-running a batch is
/// a no-op and the cursor only advances for users actually migrated.
///
/// # Resumability
/// [`MigrationState`] is persisted after the batch so the off-chain driver can
/// resume from `users_done` on the next call.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn migrate_users(e: Env, users: Vec<Address>) {
    let upgrader = read_admin(&e);
    // Upgrader role gates the migration; the admin implicitly holds it.
    require_role(&e, Role::Upgrader, &upgrader);

    let count = users.len();
    if count == 0 || count > MAX_MIGRATION_BATCH {
        panic_with_error!(e, ContractError::InvalidAmount);
    }

    let mut state = read_migration_state(&e);
    let mut migrated: u32 = 0;

    for user in users.iter() {
        if is_user_migrated(&e, &user) {
            // Already migrated: idempotent no-op.
            continue;
        }
        // Rebuild the v2 single UserIndex entry for this user.
        e.storage()
            .persistent()
            .set(&DataKey::UserIndex(user.clone()), &true);
        // Delete the legacy v1 per-user keys now that the index is rebuilt.
        e.storage()
            .persistent()
            .remove(&DataKey::LegacyUserWraps(user.clone()));
        e.storage()
            .persistent()
            .remove(&DataKey::LegacyUserRoles(user.clone()));
        migrated += 1;
    }

    state.version = 2;
    state.users_done = state.users_done.saturating_add(migrated);
    e.storage().instance().set(&DataKey::MigrationState, &state);
    e.storage().instance().set(&DataKey::SchemaVersion, &2u32);

    e.events().publish(
        (symbol_short!("migrate"), symbol_short!("users")),
        (state.version, state.users_done),
    );
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
        .set(&DataKey::MigrationVersion, &version);
}

pub(crate) fn migration_version(e: &Env) -> u32 {
    e.storage()
        .instance()
        .get(&DataKey::MigrationVersion)
        .unwrap_or(0)
}

/// Applies a WASM upgrade: bumps the `ContractVersion` counter, emits the
/// `("upgrade", version)` audit event carrying the new WASM hash, and swaps
/// the deployed code.
///
/// Shared by the direct [`upgrade`] entrypoint and the timelocked
/// `TimelockAction::Upgrade` path so both keep the version counter and the
/// event shape in sync.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn apply_upgrade(e: &Env, new_wasm_hash: BytesN<32>) {
    // Bump the contract version to track upgrade history
    let next_version: u32 = e
        .storage()
        .instance()
        .get(&DataKey::ContractVersion)
        .unwrap_or(0)
        + 1;
    e.storage()
        .instance()
        .set(&DataKey::ContractVersion, &next_version);

    // Emit audit event with the requested WASM hash and new version
    e.events().publish(
        (symbol_short!("upgrade"), next_version),
        new_wasm_hash.clone(),
    );

    // Update the contract WASM with the provided hash
    e.deployer().update_current_contract_wasm(new_wasm_hash);
}

/// Immediate WASM upgrade.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::Upgrade(..))` followed by `execute`.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin = read_admin(&e);

    current_admin.require_auth();

    apply_upgrade(&e, new_wasm_hash);
}

/// Step one of the two-step handover. Also disabled by the timelock, since an
/// immediately-acceptable proposal would otherwise bypass the delay.
pub(crate) fn propose_admin(e: Env, new_admin: Address) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin = read_admin(&e);

    current_admin.require_auth();

    if e.storage().instance().has(&DataKey::PendingAdmin) {
        panic_with_error!(e, ContractError::AdminTransferProposalExists);
    }

    e.storage()
        .instance()
        .set(&DataKey::PendingAdmin, &new_admin);
}

/// Step two of the two-step handover. Blocked while the timelock is enabled so
/// a proposal made beforehand cannot be cashed in without the delay; use
/// `cancel_proposed_admin` and reschedule through the controller instead.
pub(crate) fn accept_admin(e: Env) {
    crate::timelock::require_direct_call_allowed(&e);
    let _ = read_admin(&e);

    let pending_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::PendingAdmin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NoAdminTransferProposal));

    pending_admin.require_auth();

    e.storage().instance().set(&DataKey::Admin, &pending_admin);
    e.storage().instance().remove(&DataKey::PendingAdmin);
}

pub(crate) fn cancel_proposed_admin(e: Env) {
    let current_admin = read_admin(&e);

    current_admin.require_auth();

    if !e.storage().instance().has(&DataKey::PendingAdmin) {
        panic_with_error!(e, ContractError::NoAdminTransferProposal);
    }

    e.storage().instance().remove(&DataKey::PendingAdmin);
}

pub(crate) fn get_pending_admin(e: Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::PendingAdmin)
}

/// Immediate signing key replacement.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::SetAdminPubKey(..))` followed by `execute`.
/// In standard operation, prefer two-step rotation ([`propose_signing_key`] and
/// [`accept_signing_key`]) to cryptographically prove key possession before activation.
pub(crate) fn update_admin_pubkey(e: Env, new_pubkey: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin = read_admin(&e);
    current_admin.require_auth();

    if new_pubkey == BytesN::from_array(&e, &[0u8; 32]) {
        panic_with_error!(e, ContractError::InvalidAdminPubKey);
    }

    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &new_pubkey);
    e.storage()
        .instance()
        .remove(&DataKey::PendingAdminPubKey);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("pubkey"),
            symbol_short!("rotate"),
        ),
        new_pubkey,
    );
}

/// Step one of the two-step signing key rotation.
///
/// The current admin proposes a new Ed25519 public key. Increments the rotation
/// nonce so each proposal produces a unique challenge. Disabled when the
/// timelock is enabled.
pub(crate) fn propose_signing_key(e: Env, new_pubkey: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin = read_admin(&e);
    current_admin.require_auth();

    if new_pubkey == BytesN::from_array(&e, &[0u8; 32]) {
        panic_with_error!(e, ContractError::InvalidAdminPubKey);
    }

    if e.storage().instance().has(&DataKey::PendingAdminPubKey) {
        panic_with_error!(e, ContractError::AdminTransferProposalExists);
    }

    let nonce: u64 = e
        .storage()
        .instance()
        .get(&DataKey::SigningKeyNonce)
        .unwrap_or(0);
    let next_nonce = nonce
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ArithmeticOverflow));

    e.storage()
        .instance()
        .set(&DataKey::SigningKeyNonce, &next_nonce);
    e.storage()
        .instance()
        .set(&DataKey::PendingAdminPubKey, &new_pubkey);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("pubkey"),
            symbol_short!("propose"),
        ),
        (new_pubkey, next_nonce),
    );
}

/// Step two of the two-step signing key rotation.
///
/// Verifies the signature from the proposed key over the canonical challenge
/// containing the contract ID and proposal nonce. If valid, activates the new key
/// and clears the pending proposal. Blocked when the timelock is enabled.
pub(crate) fn accept_signing_key(e: Env, signature: BytesN<64>) {
    crate::timelock::require_direct_call_allowed(&e);
    let _ = read_admin(&e);

    let pending_pubkey: BytesN<32> = e
        .storage()
        .instance()
        .get(&DataKey::PendingAdminPubKey)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NoAdminTransferProposal));

    let nonce: u64 = e
        .storage()
        .instance()
        .get(&DataKey::SigningKeyNonce)
        .unwrap_or(0);

    let contract_id = e.current_contract_address();

    if let Err(err) = crate::signature::verify_signing_key_signature(
        &e,
        &pending_pubkey,
        &contract_id,
        nonce,
        &signature,
    ) {
        panic_with_error!(e, err);
    }

    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &pending_pubkey);
    e.storage()
        .instance()
        .remove(&DataKey::PendingAdminPubKey);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("pubkey"),
            symbol_short!("rotate"),
        ),
        pending_pubkey,
    );
}

/// Cancel an in-flight signing key proposal.
pub(crate) fn cancel_proposed_signing_key(e: Env) {
    let current_admin = read_admin(&e);
    current_admin.require_auth();

    if !e.storage().instance().has(&DataKey::PendingAdminPubKey) {
        panic_with_error!(e, ContractError::NoAdminTransferProposal);
    }

    e.storage()
        .instance()
        .remove(&DataKey::PendingAdminPubKey);
}

/// Returns the pending signing public key, if any.
pub(crate) fn get_pending_signing_key(e: Env) -> Option<BytesN<32>> {
    e.storage().instance().get(&DataKey::PendingAdminPubKey)
}

/// Returns the current signing key rotation nonce.
pub(crate) fn get_signing_key_nonce(e: Env) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::SigningKeyNonce)
        .unwrap_or(0)
}

pub(crate) fn set_name(e: Env, name: soroban_sdk::String) {
    let current_admin: Address = read_admin(&e);
    current_admin.require_auth();
    e.storage().instance().set(&DataKey::Name, &name);
}

pub(crate) fn set_symbol(e: Env, symbol: soroban_sdk::String) {
    let current_admin: Address = read_admin(&e);
    current_admin.require_auth();
    e.storage().instance().set(&DataKey::Symbol, &symbol);
}

/// Admin-only: set metadata (description and image URL) for a specific wrap.
///
/// The wrap must already exist. Both strings are length-limited to prevent
/// storage abuse. Emits an event; the storage layer accounts for the changed
/// record size automatically.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn set_wrap_metadata(
    e: Env,
    user: Address,
    period: u64,
    description: soroban_sdk::String,
    image_url: soroban_sdk::String,
) {
    read_admin(&e).require_auth();

    const MAX_DESCRIPTION_LEN: u32 = 256;
    const MAX_IMAGE_URL_LEN: u32 = 512;

    if description.len() > MAX_DESCRIPTION_LEN || image_url.len() > MAX_IMAGE_URL_LEN {
        panic_with_error!(e, ContractError::InvalidFeeParams);
    }

    let key = DataKey::Wrap(user.clone(), period);
    if !e.storage().persistent().has(&key) {
        panic_with_error!(e, ContractError::NotInitialized);
    }

    let mut record: WrapRecord = e.storage().persistent().get(&key).unwrap();
    record.description = Some(description.clone());
    record.image_url = Some(image_url.clone());

    e.storage().persistent().set(&key, &record);
    e.storage().persistent().extend_ttl(&key, TTL_TEMP, TTL_TEMP);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("admin"),
            symbol_short!("metadata"),
        ),
        (user, period, description, image_url),
    );
}

/// Validates an admin proposal duration and computes the proposal end time.
///
/// Panics with [`ContractError::InvalidProposalDuration`] if `duration_seconds`
/// is outside [`MIN_PROPOSAL_DURATION`]..=[`MAX_PROPOSAL_DURATION`] or if the
/// end timestamp overflows.
pub(crate) fn proposal_end_time(e: &Env, duration_seconds: u64) -> u64 {
    if duration_seconds < MIN_PROPOSAL_DURATION || duration_seconds > MAX_PROPOSAL_DURATION {
        panic_with_error!(e, ContractError::InvalidProposalDuration);
    }
    let start_time = e.ledger().timestamp();
    start_time
        .checked_add(duration_seconds)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::InvalidProposalDuration))
}
