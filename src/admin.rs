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
    let admin = read_admin(&e);
    admin.require_auth();
    e.storage().instance().set(&DataKey::Admin, &new_admin);
    crate::events::publish_event(&e, crate::events::Event::AdminUpdated(new_admin));
}
