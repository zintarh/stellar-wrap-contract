//! On-chain storage accounting + fee function.
//! Conservative byte estimates are used (see STORAGE.md).
use soroban_sdk::{panic_with_error, Env};

use crate::{storage_types::FeeParams, ContractError, DataKey};

/// Conservative estimates (in bytes) for persistent entries.
/// These are conservative rounded values to avoid undercharging.
const ESTIMATE_WRAP_RECORD_BYTES: u64 = 64; // conservative (48 + symbol/key overhead)
const ESTIMATE_WRAP_KEY_BYTES: u64 = 48; // enum + address + u64 rounded
const ESTIMATE_WRAP_COUNT_ENTRY_BYTES: u64 = 16; // key + u32 value overhead
const ESTIMATE_LATEST_ENTRY_BYTES: u64 = 16;
const ESTIMATE_USERPERIODS_ENTRY_BYTES: u64 = 64; // vector overhead (conservative)
const ESTIMATE_LASTUPDATED_ENTRY_BYTES: u64 = 16; // key + u64 value overhead

/// XDR string overhead: 4 bytes length prefix (uint32) + 1 byte discriminant delta.
/// The discriminant for None is already accounted for in the base record size.
const METADATA_STRING_OVERHEAD : u64 = 4;

/// Read current estimated storage bytes (instance storage)
pub(crate) fn get_storage_bytes(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::StorageBytes)
        .unwrap_or(0u64)
}

fn set_storage_bytes(e: &Env, v: u64) {
    e.storage().instance().set(&DataKey::StorageBytes, &v);
}

pub(crate) fn add_storage_bytes(e: &Env, delta: u64) {
    let cur = get_storage_bytes(e);
    let nxt = cur
        .checked_add(delta)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ArithmeticOverflow));
    set_storage_bytes(e, nxt);
}

pub(crate) fn sub_storage_bytes(e: &Env, delta: u64) {
    let cur = get_storage_bytes(e);
    let nxt = cur.saturating_sub(delta);
    set_storage_bytes(e, nxt);
}

/// Fee params helpers
pub(crate) fn get_fee_params(e: &Env) -> FeeParams {
    e.storage()
        .instance()
        .get(&DataKey::FeeParams)
        .unwrap_or(FeeParams {
            base_fee: 0,
            per_kib_fee: 0,
            scale_step_kib: 1,
            max_fee: i128::MAX,
        })
}

pub(crate) fn set_fee_params(e: &Env, params: FeeParams) {
    // enforce admin
    crate::admin::read_admin(e).require_auth();
    // validation: scale_step_kib must be non-zero (division by zero in fee computation)
    if params.scale_step_kib == 0 {
        panic_with_error!(e, ContractError::InvalidFeeParams);
    }
    // validation: fees must be non-negative
    if params.base_fee < 0 || params.per_kib_fee < 0 {
        panic_with_error!(e, ContractError::InvalidFeeParams);
    }
    // validation: max_fee must be at least as large as base_fee
    if params.max_fee < params.base_fee {
        panic_with_error!(e, ContractError::InvalidFeeParams);
    }
    e.storage().instance().set(&DataKey::FeeParams, &params);
    crate::events::publish_event(e, crate::events::Event::FeeParamsUpdated(params));
}

/// Compute the current fee according to the params and current storage bytes.
/// Model: fee = base_fee + per_kib_fee * ceil(storage_bytes / 1024)
/// capped by max_fee.
pub(crate) fn compute_current_fee(e: &Env) -> i128 {
    let params = get_fee_params(e);
    let bytes = get_storage_bytes(e);
    // KiB rounding (ceil)
    let kib = bytes.div_ceil(1024);
    // steps = kib / scale_step_kib, rounding up
    let steps = (kib + params.scale_step_kib.saturating_sub(1)) / params.scale_step_kib;
    let increment = params.per_kib_fee.saturating_mul(steps as i128);
    let mut fee = params.base_fee.saturating_add(increment);
    if fee > params.max_fee {
        fee = params.max_fee;
    }
    fee
}

/// Convenience: estimate bytes added for creating a wrap and associated auxiliary keys.
pub(crate) fn estimate_wrap_bytes_new() -> u64 {
    ESTIMATE_WRAP_RECORD_BYTES + ESTIMATE_WRAP_KEY_BYTES
}

pub(crate) fn estimate_wrapcount_bytes_new() -> u64 {
    ESTIMATE_WRAP_COUNT_ENTRY_BYTES
}

pub(crate) fn estimate_latest_bytes_new() -> u64 {
    ESTIMATE_LATEST_ENTRY_BYTES
}

pub(crate) fn estimate_userperiods_bytes_new() -> u64 {
    ESTIMATE_USERPERIODS_ENTRY_BYTES
}

pub(crate) fn estimate_lastupdated_bytes_new() -> u64 {
    ESTIMATE_LASTUPDATED_ENTRY_BYTES
}
