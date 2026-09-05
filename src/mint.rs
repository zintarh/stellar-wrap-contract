use soroban_sdk::{panic_with_error, symbol_short, Address, BytesN, Env, Symbol};

use crate::{
    optout,
    optout,
    signature::verify_mint_signature,
    storage_accounting,
    storage_types::{WrapLifecycleFSM, WrapState},
    ttl::TTL_ONE_YEAR,
    ContractError, DataKey, WrapRecord,
};

pub const CURRENT_PAYLOAD_VERSION: u32 = 1;
/// Default expiration duration for unverified wraps: 7 days in seconds.
const DEFAULT_EXPIRATION_SECONDS: u64 = 7 * 24 * 60 * 60;
pub const MIN_PERIOD_YEAR: u64 = 2024;
pub const MAX_PERIOD_YEAR: u64 = 2100;

pub fn validate_period(e: &Env, period: u64) {
    let year = period / 100;
    let month = period % 100;

    if !(MIN_PERIOD_YEAR..=MAX_PERIOD_YEAR).contains(&year) || !(1..=12).contains(&month) {
        panic_with_error!(e, ContractError::InvalidPeriod);
    }
}

fn validate_payload_version(e: &Env, version: u32) {
    if version != CURRENT_PAYLOAD_VERSION {
        panic_with_error!(e, ContractError::InvalidSignature);
    }
}

fn get_admin_pubkey(e: &Env) -> BytesN<32> {
    e.storage()
        .instance()
        .get(&DataKey::AdminPubKey)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized))
}

/// Records the ledger timestamp at which the user's registry state last
/// changed via a successful mint or revoke. The value is monotonic per user.
///
/// Storage bytes are accounted once, when the entry is first created; updates
/// only overwrite the existing value and renew its TTL.
pub(crate) fn update_last_updated(e: &Env, user: &Address) {
    let now = e.ledger().timestamp();
    let key = DataKey::LastUpdated(user.clone());
    let was_missing = !e.storage().persistent().has(&key);
    e.storage().persistent().set(&key, &now);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    if was_missing {
        storage_accounting::add_storage_bytes(
            e,
            storage_accounting::estimate_lastupdated_bytes_new(),
        );
    }
}

/// Updates the latest period for a user if `period` is greater than the currently stored latest period (if any).
///
/// Storage bytes are accounted once, when the entry is first created; subsequent updates
/// only overwrite the existing value if `period` is higher and renew its TTL.
pub(crate) fn update_latest_period(e: &Env, user: &Address, period: u64) {
    let latest_key = DataKey::LatestPeriod(user.clone());
    let current_latest: Option<u64> = e.storage().persistent().get(&latest_key);
    let should_update = match current_latest {
        Some(cur) => period > cur,
        None => true,
    };
    if should_update {
        let was_missing = current_latest.is_none();
        e.storage().persistent().set(&latest_key, &period);
        e.storage()
            .persistent()
            .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        if was_missing {
            storage_accounting::add_storage_bytes(
                e,
                storage_accounting::estimate_latest_bytes_new(),
            );
        }
    }
}

/// Inserts a new wrap record and maintains all associated per-user indexes and storage accounting.
///
/// This helper unifies index maintenance across `mint::mint_wrap`, `mint::mint_wrap_batch`,
/// and `bridge::bridge_wrap_in`.
///
/// # Invariants and Index Maintenance
/// - **Opt-out Validation**: Panics with `ContractError::UserOptedOut` if the user has opted out (`DataKey::OptOut`).
/// - **Opt-out Validation**: Panics with `ContractError::UserOptedOut` if the user has opted out (`DataKey::OptOut`).
/// - **Existence Check**: Panics with `ContractError::WrapAlreadyExists` if a record already exists at `DataKey::Wrap(user, period)`.
/// - **Storage & TTL**: Writes `DataKey::Wrap(user, period)` and extends TTL (1 year).
/// - **Storage Accounting**: Adds estimated storage bytes for the new wrap record (`estimate_wrap_bytes_new`).
/// - **Wrap Counts**: Increments `DataKey::WrapCount(user)` and `DataKey::TotalWrapCount`, extending TTL. If this is the user's first wrap, accounts for `estimate_wrapcount_bytes_new`.
/// - **Latest Period**: Updates `DataKey::LatestPeriod(user)` if `period` is newer than the recorded latest period.
/// - **User Periods**: Appends `period` to `DataKey::UserPeriods(user)` if not already present, extending TTL and accounting for `estimate_userperiods_bytes_new`.
/// - **Legacy Index Invariant**: If `WrapCount > 0` but `DataKey::WrapPeriods(user)` is missing, panics with `ContractError::StorageInvariantViolation` (requiring backfill).
/// - **Wrap Periods**: Appends `period` to `DataKey::WrapPeriods(user)` if not already present, extending TTL.
/// - **Last Updated**: Updates `DataKey::LastUpdated(user)` to the current ledger timestamp.
///
/// # Intentional Differences Handled by Callers
/// - **Event Emission**: Callers emit their own caller-specific events:
///   - `mint_wrap`: emits typed `MintEventData::Mint` with `MintEventType::Mint`.
///   - `mint_wrap_batch`: emits typed `MintEventData::Mint` with `MintEventType::Mint`.
///   - `bridge_wrap_in`: emits `br_in` event and persists `InboundBridgeRecord`.
/// - **Pre-Validation**: Callers handle authorization (`require_auth`), paused checks (`require_not_paused`), signature verification, and replay protection prior to calling `insert_wrap_record`.
pub(crate) fn insert_wrap_record(e: &Env, user: &Address, period: u64, record: &WrapRecord) {
    optout::require_not_opted_out(e, user);

    optout::require_not_opted_out(e, user);

    let wrap_key = DataKey::Wrap(user.clone(), period);
    if e.storage().persistent().has(&wrap_key) {
        panic_with_error!(e, ContractError::WrapAlreadyExists);
    }

    e.storage().persistent().set(&wrap_key, record);
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    // Account for estimated storage bytes for new wrap record
    storage_accounting::add_storage_bytes(e, storage_accounting::estimate_wrap_bytes_new());

    // Update wrap count and account for count entry if first insert
    let count_key = DataKey::WrapCount(user.clone());
    let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
    let next_count = current_count + 1;
    e.storage().persistent().set(&count_key, &next_count);
    e.storage()
        .persistent()
        .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    let total_key = DataKey::TotalWrapCount;
    let current_total: u32 = e.storage().persistent().get(&total_key).unwrap_or(0);
    let next_total = current_total + 1;
    e.storage().persistent().set(&total_key, &next_total);
    e.storage()
        .persistent()
        .extend_ttl(&total_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    if current_count == 0 {
        storage_accounting::add_storage_bytes(
            e,
            storage_accounting::estimate_wrapcount_bytes_new(),
        );
    }

    update_latest_period(e, user, period);

    // UserPeriods: if we push a new period value, account for it
    let user_periods_key = DataKey::UserPeriods(user.clone());
    let mut periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&user_periods_key)
        .unwrap_or(soroban_sdk::Vec::new(e));

    if !periods.contains(period) {
        periods.push_back(period);
        e.storage().persistent().set(&user_periods_key, &periods);
        e.storage()
            .persistent()
            .extend_ttl(&user_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        storage_accounting::add_storage_bytes(
            e,
            storage_accounting::estimate_userperiods_bytes_new(),
        );
    }

    // WrapPeriods: transfer-index maintained alongside UserPeriods. A user that
    // already has wraps but is missing this index is a legacy user that must be
    // backfilled via `backfill_wrap_periods` before further mints are allowed.
    let wrap_periods_key = DataKey::WrapPeriods(user.clone());
    if !e.storage().persistent().has(&wrap_periods_key) && current_count > 0 {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }
    let mut wrap_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)
        .unwrap_or(soroban_sdk::Vec::new(e));
    if !wrap_periods.contains(period) {
        wrap_periods.push_back(period);
        e.storage()
            .persistent()
            .set(&wrap_periods_key, &wrap_periods);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    update_last_updated(e, user);
}

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn mint_wrap(
    e: Env,
    user: Address,
    period: u64,
    archetype: Symbol,
    data_hash: BytesN<32>,
    payload_version: u32,
    signature: BytesN<64>,
) {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    validate_period(&e, period);
    validate_payload_version(&e, payload_version);

    let admin_pubkey = get_admin_pubkey(&e);
    if let Err(err) = verify_mint_signature(
        &e,
        &admin_pubkey,
        &e.current_contract_address(),
        &user,
        period,
        &archetype,
        &data_hash,
        payload_version,
        &signature,
    ) {
        panic_with_error!(e, err);
    }

    let now = e.ledger().timestamp();
    let record = WrapRecord {
        timestamp: now,
        data_hash,
        archetype: archetype.clone(),
        period,
        fsm: WrapLifecycleFSM::new(WrapState::Active, now),
        description: None,
        image_url: None,
    };

    insert_wrap_record(&e, &user, period, &record);

    crate::events::publish_event(&e, crate::events::Event::Mint(user, period, archetype));
}

pub const MAX_BATCH_SIZE: u32 = 100;

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn mint_wrap_batch(
    e: Env,
    items: soroban_sdk::Vec<crate::storage_types::BatchWrapItem>,
    aggregated_signature: Option<BytesN<64>>,
) {
    crate::admin::require_not_paused(&e);
    if items.is_empty() {
        panic_with_error!(&e, ContractError::BatchEmpty);
    }
    if items.len() > MAX_BATCH_SIZE {
        panic_with_error!(&e, ContractError::BatchTooLarge);
    }

    // Check for duplicate (user, period) pairs within the batch before any storage writes.
    {
        let mut seen_pairs: soroban_sdk::Vec<(Address, u64)> = soroban_sdk::Vec::new(&e);
        for item in items.iter() {
            let pair = (item.user.clone(), item.period);
            for seen in seen_pairs.iter() {
                if seen.0 == pair.0 && seen.1 == pair.1 {
                    panic_with_error!(&e, ContractError::DuplicateBatchEntry);
                }
            }
            seen_pairs.push_back(pair);
        }
    }

    let admin_pubkey = get_admin_pubkey(&e);
    let contract_id = e.current_contract_address();

    let mut authed_users = soroban_sdk::Vec::<Address>::new(&e);
    for item in items.iter() {
        if !authed_users.contains(&item.user) {
            item.user.require_auth();
            authed_users.push_back(item.user.clone());
        }
    }

    if let Some(agg_sig) = aggregated_signature {
        // Validate payload version of items
        for item in items.iter() {
            validate_period(&e, item.period);
            validate_payload_version(&e, item.payload_version);
        }
        let payload_version = items.get(0).unwrap().payload_version;
        if let Err(err) = crate::signature::verify_batch_aggregated_signature(
            &e,
            &admin_pubkey,
            &contract_id,
            &items,
            payload_version,
            &agg_sig,
        ) {
            panic_with_error!(e, err);
        }
    } else {
        // Individual signatures inside batch items
        for item in items.iter() {
            validate_period(&e, item.period);
            validate_payload_version(&e, item.payload_version);

            if let Err(err) = verify_mint_signature(
                &e,
                &admin_pubkey,
                &contract_id,
                &item.user,
                item.period,
                &item.archetype,
                &item.data_hash,
                item.payload_version,
                &item.signature,
            ) {
                panic_with_error!(e, err);
            }
        }
    }

    // Process each wrap insertion
    for item in items.iter() {
        let now = e.ledger().timestamp();
        let record = WrapRecord {
            timestamp: now,
            data_hash: item.data_hash.clone(),
            archetype: item.archetype.clone(),
            period: item.period,
            fsm: WrapLifecycleFSM::new(WrapState::Active, now),
            description: None,
            image_url: None,
        };

        insert_wrap_record(&e, &item.user, item.period, &record);

        e.events().publish(
            (
                MintEventType::Mint.to_symbol(&e),
                item.user.clone(),
                item.period,
            ),
            MintEventData::Mint(item.user, item.period, item.archetype),
        );
    }
}

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn transition_wrap_state(e: Env, user: Address, period: u64, next_state: WrapState) {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    let wrap_key = DataKey::Wrap(user.clone(), period);
    let mut record: WrapRecord = e
        .storage()
        .persistent()
        .get(&wrap_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

    if record.fsm.state == WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    let now = e.ledger().timestamp();
    if !record.fsm.transition_to(next_state, now) {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    e.storage().persistent().set(&wrap_key, &record);
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    crate::events::publish_event(
        &e,
        crate::events::Event::MintTransition(user, period, next_state),
    );
}

// ─── Expiration mechanism ────────────────────────────────────────────────

/// Returns the configured expiration duration for unverified wraps.
/// Defaults to 7 days (604,800 seconds) if not set by admin.
pub(crate) fn get_expiration_duration(e: &Env) -> u64 {
    e.storage()
        .instance()
        .get(&DataKey::ExpirationDuration)
        .unwrap_or(DEFAULT_EXPIRATION_SECONDS)
}

/// Admin-only: sets the expiration duration (in seconds) for unverified wraps.
/// Wraps in Draft or Pending state that remain unverified beyond this duration
/// can be expired by anyone via [`expire_wrap`].
pub(crate) fn set_expiration_duration(e: &Env, duration: u64) {
    crate::admin::read_admin(e).require_auth();
    if duration == 0 {
        panic_with_error!(e, ContractError::InvalidExpirationDuration);
    }
    e.storage()
        .instance()
        .set(&DataKey::ExpirationDuration, &duration);
}

/// Expires a wrap if its expiration deadline has passed.
///
/// A wrap can be expired if:
/// - It is in `Draft`, `Pending`, or `Active` state.
/// - The ledger timestamp exceeds `fsm.updated_at + expiration_duration`.
///
/// Callable by anyone — the function enforces objective time-based criteria.
/// Wraps already in `Archived`, `Cancelled`, or `Expired` state
/// will cause the FSM transition to fail with [`ContractError::InvalidStateTransition`].
///
/// Expired wraps remain in persistent storage; no storage bytes are reclaimed.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn expire_wrap(e: Env, user: Address, period: u64) {
    crate::admin::require_not_paused(&e);

    let wrap_key = DataKey::Wrap(user.clone(), period);
    let mut record: WrapRecord = e
        .storage()
        .persistent()
        .get(&wrap_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

    let now = e.ledger().timestamp();
    let duration = get_expiration_duration(&e);
    let expires_at = record.fsm.updated_at.saturating_add(duration);

    if now <= expires_at {
        panic_with_error!(e, ContractError::WrapNotExpired);
    }

    if !record.fsm.transition_to(WrapState::Expired, now) {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    e.storage().persistent().set(&wrap_key, &record);
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    e.events().publish(
        (symbol_short!("expire"), user, period),
        symbol_short!("expired"),
    );
}