use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env};

use crate::{ttl::TTL_TEMP, ContractError, DataKey, TransferFeeConfig};

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

pub(crate) fn update_admin_pubkey(e: Env, new_pubkey: BytesN<32>) {
    let current_admin = read_admin(&e);
    current_admin.require_auth();

    if new_pubkey == BytesN::from_array(&e, &[0u8; 32]) {
        panic_with_error!(e, ContractError::InvalidAdminPubKey);
    }

    e.storage()
        .instance()
        .set(&DataKey::AdminPubKey, &new_pubkey);

    e.events().publish(
        (
            symbol_short!("v1"),
            symbol_short!("pubkey"),
            symbol_short!("rotate"),
        ),
        new_pubkey,
    );
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

    let mut record: WrapRecord = e
        .storage()
        .persistent()
        .get(&key)
        .unwrap();
    record.description = Some(description.clone());
    record.image_url = Some(image_url.clone());

    e.storage().persistent().set(&key, &record);
    crate::ttl::extend_ttl(&e, &key, TTL_TEMP, TTL_TEMP);

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
