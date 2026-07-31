use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::mint::TTL_TEMP;
use crate::{ContractError, DataKey};

/// Reads the stored admin or panics with `NotInitialized`.
pub(crate) fn read_admin(e: &Env) -> Address {
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

/// Rejects obviously invalid Ed25519 signing public keys that would otherwise
/// make every signature check trivially forgeable or unusable:
/// the all-zero key and the identity (neutral) point.
fn is_invalid_admin_pubkey(pubkey: &BytesN<32>) -> bool {
    let bytes = pubkey.to_array();
    let is_all_zero = bytes.iter().all(|b| *b == 0);
    let is_identity = bytes[0] == 1 && bytes[1..].iter().all(|b| *b == 0);
    is_all_zero || is_identity
}

pub(crate) fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
    if e.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(e, ContractError::AlreadyInitialized);
    }
    if is_invalid_admin_pubkey(&admin_pubkey) {
        panic_with_error!(e, ContractError::InvalidAdminPubkey);
    }
    e.storage().instance().set(&DataKey::Admin, &admin);
    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &admin_pubkey);
    e.events().publish((symbol_short!("init"),), admin);
}

/// Immediate admin replacement.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::SetAdmin(..))` followed by `execute`.
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

pub(crate) fn set_pause(e: Env, paused: bool) {
    read_admin(&e).require_auth();
    e.storage().instance().set(&DataKey::Paused, &paused);
    e.events().publish((symbol_short!("pause"),), paused);
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

/// Immediate WASM upgrade.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::Upgrade(..))` followed by `execute`.
pub(crate) fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    current_admin.require_auth();

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
    e.events()
        .publish((symbol_short!("upgrade"), next_version), new_wasm_hash.clone());

    // Update the contract WASM with the provided hash
    e.deployer().update_current_contract_wasm(new_wasm_hash);
}

/// Step one of the two-step handover. Also disabled by the timelock, since an
/// immediately-acceptable proposal would otherwise bypass the delay.
pub(crate) fn propose_admin(e: Env, new_admin: Address) {
    crate::timelock::require_direct_call_allowed(&e);
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

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
    let _: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

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
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    current_admin.require_auth();

    if !e.storage().instance().has(&DataKey::PendingAdmin) {
        panic_with_error!(e, ContractError::NoAdminTransferProposal);
    }

    e.storage().instance().remove(&DataKey::PendingAdmin);
}

pub(crate) fn get_pending_admin(e: Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::PendingAdmin)
}

pub(crate) fn set_name(e: Env, name: soroban_sdk::String) {
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    current_admin.require_auth();
    e.storage().temporary().set(&DataKey::Name, &name);
    e.storage()
        .temporary()
        .extend_ttl(&DataKey::Name, TTL_TEMP, TTL_TEMP);
}

pub(crate) fn set_symbol(e: Env, symbol: soroban_sdk::String) {
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    current_admin.require_auth();
    e.storage().temporary().set(&DataKey::Symbol, &symbol);
    e.storage()
        .temporary()
        .extend_ttl(&DataKey::Symbol, TTL_TEMP, TTL_TEMP);
}
