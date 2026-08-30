use soroban_sdk::{panic_with_error, symbol_short, Address, Bytes, BytesN, Env, Symbol};

use crate::storage_types::{
    InboundBridgeRecord, OutboundBridgeRequest, WrapLifecycleFSM, WrapRecord, WrapState,
};
use crate::{storage_accounting, ContractError, DataKey};

const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Set the bridge relayer address. Requires admin authorization.
pub(crate) fn set_bridge_relayer(e: &Env, relayer: Address) {
    let admin = crate::admin::read_admin(e);
    admin.require_auth();
    e.storage()
        .instance()
        .set(&DataKey::BridgeRelayer, &relayer);
    e.storage()
        .instance()
        .extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Returns the configured bridge relayer address, or None if not set.
pub(crate) fn get_bridge_relayer(e: &Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::BridgeRelayer)
}

/// Enable or disable a cross-chain network chain ID. Requires admin authorization.
pub(crate) fn set_chain_status(e: &Env, chain_id: u32, enabled: bool) {
    let admin = crate::admin::read_admin(e);
    admin.require_auth();
    if chain_id == 0 {
        panic_with_error!(e, ContractError::InvalidChain);
    }
    let key = DataKey::BridgeChainStatus(chain_id);
    e.storage().instance().set(&key, &enabled);
    e.storage()
        .instance()
        .extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Returns whether a target/source chain ID is supported/enabled.
pub(crate) fn is_chain_supported(e: &Env, chain_id: u32) -> bool {
    if chain_id == 0 {
        return false;
    }
    let key = DataKey::BridgeChainStatus(chain_id);
    e.storage().instance().get(&key).unwrap_or(false)
}

/// Initiate an outbound cross-chain token/wrap bridge transfer.
/// User locks/bridges their wrap record to a destination chain.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn bridge_wrap_out(
    e: Env,
    user: Address,
    destination_chain: u32,
    recipient_address: Bytes,
    period: u64,
) -> u64 {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    if !is_chain_supported(&e, destination_chain) {
        panic_with_error!(e, ContractError::ChainDisabled);
    }

    if recipient_address.is_empty() {
        panic_with_error!(e, ContractError::InvalidBridgePayload);
    }

    let wrap_key = DataKey::Wrap(user.clone(), period);
    let mut wrap_record: WrapRecord = e
        .storage()
        .persistent()
        .get(&wrap_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

    let now = e.ledger().timestamp();

    if !wrap_record.fsm.transition_to(WrapState::Bridged, now) {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }
    e.storage().persistent().set(&wrap_key, &wrap_record);
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    let nonce_key = DataKey::OutboundBridgeNonce;
    let current_nonce: u64 = e.storage().instance().get(&nonce_key).unwrap_or(0);
    let next_nonce = current_nonce + 1;
    e.storage().instance().set(&nonce_key, &next_nonce);
    e.storage()
        .instance()
        .extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);

    let outbound_req = OutboundBridgeRequest {
        nonce: next_nonce,
        sender: user.clone(),
        destination_chain,
        recipient_address: recipient_address.clone(),
        period,
        archetype: wrap_record.archetype.clone(),
        data_hash: wrap_record.data_hash.clone(),
        timestamp: now,
    };

    let req_key = DataKey::OutboundBridgeRequest(next_nonce);
    e.storage().persistent().set(&req_key, &outbound_req);
    e.storage()
        .persistent()
        .extend_ttl(&req_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    storage_accounting::add_storage_bytes(&e, 128);

    e.events().publish(
        (symbol_short!("br_out"), user, destination_chain),
        (next_nonce, recipient_address, period),
    );

    next_nonce
}

/// Restore a bridged wrap when the destination chain rejects the transfer.
/// Only the configured bridge relayer may call this operation.
pub(crate) fn bridge_wrap_refund(e: Env, outbound_nonce: u64) {
    crate::admin::require_not_paused(&e);

    let relayer = get_bridge_relayer(&e)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BridgeNotInitialized));
    relayer.require_auth();

    let request_key = DataKey::OutboundBridgeRequest(outbound_nonce);
    let request: OutboundBridgeRequest = e
        .storage()
        .persistent()
        .get(&request_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::InvalidBridgePayload));
    let wrap_key = DataKey::Wrap(request.sender.clone(), request.period);
    let mut wrap_record: WrapRecord = e
        .storage()
        .persistent()
        .get(&wrap_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

    let now = e.ledger().timestamp();
    if !wrap_record.fsm.restore_from_bridge(now) {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    e.storage().persistent().set(&wrap_key, &wrap_record);
    e.storage()
        .persistent()
        .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    e.events().publish(
        (symbol_short!("br_refund"), request.sender.clone(), request.period),
        outbound_nonce,
    );
}

/// Fulfill an inbound cross-chain token/wrap bridge transfer.
/// Called by authorized relayer to process wraps coming from an external chain.
///
/// An opted-out recipient rejects the message without creating or updating any
/// wrap records. The inbound nonce is still consumed and a `br_in_rej` event
/// is emitted so the relayer does not retry the rejected message indefinitely.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn bridge_wrap_in(
    e: Env,
    source_chain: u32,
    source_nonce: u64,
    recipient: Address,
    period: u64,
    archetype: Symbol,
    data_hash: BytesN<32>,
) {
    crate::admin::require_not_paused(&e);

    let relayer = get_bridge_relayer(&e)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BridgeNotInitialized));
    relayer.require_auth();

    if !is_chain_supported(&e, source_chain) {
        panic_with_error!(e, ContractError::ChainDisabled);
    }

    let processed_key = DataKey::InboundBridgeProcessed(source_chain, source_nonce);
    let is_processed: bool = e
        .storage()
        .persistent()
        .get(&processed_key)
        .unwrap_or(false);

    if is_processed {
        panic_with_error!(e, ContractError::NonceAlreadyProcessed);
    }

    crate::mint::validate_period(&e, period);

    e.storage().persistent().set(&processed_key, &true);
    e.storage()
        .persistent()
        .extend_ttl(&processed_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    if e.storage()
        .persistent()
        .has(&DataKey::OptOut(recipient.clone()))
    {
        e.events().publish(
            (symbol_short!("br_in_rej"), recipient, source_chain),
            (source_nonce, period),
        );
        return;
    }

    let now = e.ledger().timestamp();
    let wrap_key = DataKey::Wrap(recipient.clone(), period);

    if !e.storage().persistent().has(&wrap_key) {
        let record = WrapRecord {
            timestamp: now,
            data_hash: data_hash.clone(),
            archetype: archetype.clone(),
            period,
            fsm: WrapLifecycleFSM::new(WrapState::Active, now),
            description: None,
            image_url: None,
        };

        e.storage().persistent().set(&wrap_key, &record);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        storage_accounting::add_storage_bytes(&e, storage_accounting::estimate_wrap_bytes_new());

        let count_key = DataKey::WrapCount(recipient.clone());
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
                &e,
                storage_accounting::estimate_wrapcount_bytes_new(),
            );
        }

        crate::mint::update_latest_period(&e, &recipient, period);

        let user_periods_key = DataKey::UserPeriods(recipient.clone());
        let mut periods: soroban_sdk::Vec<u64> = e
            .storage()
            .persistent()
            .get(&user_periods_key)
            .unwrap_or(soroban_sdk::Vec::new(&e));

        if !periods.contains(period) {
            periods.push_back(period);
            e.storage().persistent().set(&user_periods_key, &periods);
            e.storage()
                .persistent()
                .extend_ttl(&user_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

            storage_accounting::add_storage_bytes(
                &e,
                storage_accounting::estimate_userperiods_bytes_new(),
            );
        }

        let wrap_periods_key = DataKey::WrapPeriods(recipient.clone());
        let mut wrap_periods: soroban_sdk::Vec<u64> = e
            .storage()
            .persistent()
            .get(&wrap_periods_key)
            .unwrap_or(soroban_sdk::Vec::new(&e));

        if !wrap_periods.contains(period) {
            wrap_periods.push_back(period);
            e.storage()
                .persistent()
                .set(&wrap_periods_key, &wrap_periods);
            e.storage()
                .persistent()
                .extend_ttl(&wrap_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    } else {
        let mut existing_record: WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();
        if !existing_record.fsm.transition_to(WrapState::Active, now) {
            panic_with_error!(e, ContractError::InvalidStateTransition);
        }
        e.storage().persistent().set(&wrap_key, &existing_record);

        e.events().publish(
            (
                crate::events::MintEventType::Transition.to_symbol(&e),
                recipient.clone(),
                period,
            ),
            crate::events::MintEventData::Transition(recipient.clone(), period, WrapState::Active),
        );
    }

    let wrap_periods_key = DataKey::WrapPeriods(recipient.clone());
    let mut wrap_periods: soroban_sdk::Vec<u64> = e
        .storage()
        .persistent()
        .get(&wrap_periods_key)
        .unwrap_or(soroban_sdk::Vec::new(&e));
    if !wrap_periods.contains(period) {
        wrap_periods.push_back(period);
        e.storage()
            .persistent()
            .set(&wrap_periods_key, &wrap_periods);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }

    let wrap_count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::WrapCount(recipient.clone()))
        .unwrap_or(0);
    if wrap_count != wrap_periods.len() {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }

    let inbound_rec = InboundBridgeRecord {
        source_chain,
        source_nonce,
        recipient: recipient.clone(),
        period,
        archetype,
        data_hash,
        timestamp: now,
    };

    let record_key = DataKey::InboundBridgeRecord(source_chain, source_nonce);
    e.storage().persistent().set(&record_key, &inbound_rec);
    e.storage()
        .persistent()
        .extend_ttl(&record_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    e.events().publish(
        (symbol_short!("br_in"), recipient, source_chain),
        (source_nonce, period),
    );
}

pub(crate) fn get_outbound_bridge_request(e: &Env, nonce: u64) -> Option<OutboundBridgeRequest> {
    let key = DataKey::OutboundBridgeRequest(nonce);
    e.storage().persistent().get(&key)
}

pub(crate) fn get_inbound_bridge_record(
    e: &Env,
    source_chain: u32,
    source_nonce: u64,
) -> Option<InboundBridgeRecord> {
    let key = DataKey::InboundBridgeRecord(source_chain, source_nonce);
    e.storage().persistent().get(&key)
}

pub(crate) fn is_inbound_nonce_processed(e: &Env, source_chain: u32, source_nonce: u64) -> bool {
    let key = DataKey::InboundBridgeProcessed(source_chain, source_nonce);
    e.storage().persistent().get(&key).unwrap_or(false)
}

pub(crate) fn get_outbound_nonce(e: &Env) -> u64 {
    let key = DataKey::OutboundBridgeNonce;
    e.storage().instance().get(&key).unwrap_or(0)
}
