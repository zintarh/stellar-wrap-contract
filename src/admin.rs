use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::{ContractError, DataKey, TransferFeeConfig};

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
    admin.require_auth();
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
    e.events().publish((symbol_short!("init"),), admin);
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

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn set_pause(e: Env, paused: bool) {
    read_admin(&e).require_auth();
    e.storage().instance().set(&DataKey::Paused, &paused);
    e.events().publish((symbol_short!("pause"),), paused);
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

    // Migrate Name and Symbol from temporary storage to instance storage if present
    if let Some(name) = e.storage().temporary().get::<_, soroban_sdk::String>(&DataKey::Name) {
        e.storage().instance().set(&DataKey::Name, &name);
        e.storage().temporary().remove(&DataKey::Name);
    }
    if let Some(symbol) = e.storage().temporary().get::<_, soroban_sdk::String>(&DataKey::Symbol) {
        e.storage().instance().set(&DataKey::Symbol, &symbol);
        e.storage().temporary().remove(&DataKey::Symbol);
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

/// Emergency admin signing-key rotation.
///
/// Authorized by the current admin. Exempt from the timelock (like `pause`)
/// so a compromised key can be immediately rotated without waiting out the
/// timelock delay window (during which a compromised key would continue to
/// produce valid signatures).
pub(crate) fn update_admin_pubkey(e: Env, new_pubkey: BytesN<32>) {
    let current_admin = read_admin(&e);
    current_admin.require_auth();

    if new_pubkey == BytesN::from_array(&e, &[0u8; 32]) {
        panic_with_error!(e, ContractError::InvalidAdminPubKey);
    }

    let old_pubkey: BytesN<32> = e
        .storage()
        .instance()
        .get(&DataKey::AdminPubKey)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &new_pubkey);

    e.events().publish(
        (
            symbol_short!("pubkey"),
            symbol_short!("rotate"),
        ),
        (old_pubkey, new_pubkey),
    );
}

/// Immediate WASM upgrade.
///
/// Rejected once the timelock controller is enabled — the admin must then use
/// `schedule(TimelockAction::Upgrade(..))` followed by `execute`.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
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
    e.events().publish(
        (symbol_short!("upgrade"), next_version),
        new_wasm_hash.clone(),
    );

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
    e.storage().instance().set(&DataKey::Name, &name);
}

pub(crate) fn set_symbol(e: Env, symbol: soroban_sdk::String) {
    let current_admin: Address = e
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

    current_admin.require_auth();
    e.storage().instance().set(&DataKey::Symbol, &symbol);
}
