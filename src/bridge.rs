use soroban_sdk::{panic_with_error, symbol_short, Address, Bytes, BytesN, Env, Symbol};

use crate::{
    signature::verify_inbound_bridge_signature,
    storage_accounting,
    storage_types::{
        BridgeRelayerSet, InboundBridgeRecord, OutboundBridgeRequest, WrapLifecycleFSM, WrapRecord,
        WrapState,
    },
    ContractError, DataKey,
};

const TTL_ONE_YEAR: u32 = 17_280 * 365;

/// Set the bridge relayers for a given chain. Requires admin authorization.
pub(crate) fn set_bridge_relayers(
    e: &Env,
    chain_id: u32,
    relayers: soroban_sdk::Vec<soroban_sdk::BytesN<32>>,
    threshold: u32,
) {
    let admin = crate::admin::read_admin(e);
    admin.require_auth();
    if threshold == 0 || threshold > relayers.len() {
        panic_with_error!(e, ContractError::InvalidThreshold);
    }
    let key = DataKey::BridgeRelayerSet(chain_id);
    let relayer_set = BridgeRelayerSet {
        relayers,
        threshold,
    };
    e.storage().instance().set(&key, &relayer_set);
    e.storage()
        .instance()
        .extend_ttl(TTL_ONE_YEAR, TTL_ONE_YEAR);
}

/// Set the single bridge relayer address used to authorize refunds.
/// Requires admin authorization.
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

/// Returns the configured bridge relayer set for a given chain, or None if not set.
pub(crate) fn get_bridge_relayers(e: &Env, chain_id: u32) -> Option<BridgeRelayerSet> {
    e.storage()
        .instance()
        .get(&DataKey::BridgeRelayerSet(chain_id))
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
    let next_nonce = current_nonce
        .checked_add(1)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ArithmeticOverflow));
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
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn bridge_wrap_refund(e: Env, outbound_nonce: u64) {
    crate::admin::require_not_paused(&e);

    let request_key = DataKey::OutboundBridgeRequest(outbound_nonce);
    let request: OutboundBridgeRequest = e
        .storage()
        .persistent()
        .get(&request_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::InvalidBridgePayload));

    let _relayer_set = get_bridge_relayers(&e, request.destination_chain)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BridgeNotInitialized));
    // TODO: Verify signatures for refund. For now, we just require admin authorization as fallback.
    let admin = crate::admin::read_admin(&e);
    admin.require_auth();
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

    #[allow(deprecated)]
    e.events().publish(
        (
            symbol_short!("br_refund"),
            request.sender.clone(),
            request.period,
        ),
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn bridge_wrap_in(
    e: Env,
    source_chain: u32,
    source_nonce: u64,
    recipient: Address,
    period: u64,
    archetype: Symbol,
    data_hash: BytesN<32>,
    signatures: soroban_sdk::Vec<BytesN<64>>,
) {
    crate::admin::require_not_paused(&e);

    // An opted-out recipient must never receive a bridged wrap, matching the
    // mint path guard.
    crate::optout::require_not_opted_out(&e, &recipient);

    if !is_chain_supported(&e, source_chain) {
        panic_with_error!(e, ContractError::ChainDisabled);
    }

    let relayer_set = get_bridge_relayers(&e, source_chain)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::BridgeNotInitialized));

    if signatures.len() < relayer_set.threshold {
        panic_with_error!(e, ContractError::InvalidSignature);
    }

    let contract_id = e.current_contract_address();
    let mut verified_count = 0;
    let mut used_relayers = soroban_sdk::Vec::new(&e);

    for sig in signatures.iter() {
        let mut matched = false;
        for relayer in relayer_set.relayers.iter() {
            if used_relayers.contains(&relayer) {
                continue;
            }
            if verify_inbound_bridge_signature(
                &e,
                &relayer,
                &contract_id,
                source_chain,
                source_nonce,
                &recipient,
                period,
                &archetype,
                &data_hash,
                &sig,
            )
            .is_ok()
            {
                used_relayers.push_back(relayer);
                matched = true;
                break;
            }
        }
        if matched {
            verified_count = verified_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(e, ContractError::ArithmeticOverflow));
        } else {
            panic_with_error!(e, ContractError::InvalidSignature);
        }
    }

    if verified_count < relayer_set.threshold {
        panic_with_error!(e, ContractError::InvalidSignature);
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

        crate::mint::insert_wrap_record(&e, &recipient, period, &record);
    } else {
        let mut existing_record: WrapRecord = e.storage().persistent().get(&wrap_key).unwrap();
        if !existing_record.fsm.restore_from_bridge(now) {
            panic_with_error!(e, ContractError::InvalidStateTransition);
        }
        e.storage().persistent().set(&wrap_key, &existing_record);

        crate::events::publish_event(
            &e,
            crate::events::Event::MintTransition(recipient.clone(), period, WrapState::Active),
        );
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
