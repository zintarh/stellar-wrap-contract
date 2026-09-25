#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};

use super::*;
use crate::signature::{construct_inbound_bridge_payload, construct_mint_payload};

fn setup_test_env<'a>(
    env: &'a Env,
) -> (
    StellarWrapContractClient<'a>,
    Address,
    SigningKey,
    SigningKey,
) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let relayer_key = SigningKey::from_bytes(&[8u8; 32]);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    (client, admin, relayer_key, signing_key)
}

fn setup_relayer(
    env: &Env,
    client: &StellarWrapContractClient,
    chain_id: u32,
    relayer_key: &SigningKey,
) {
    let relayer_pubkey = BytesN::from_array(env, &relayer_key.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(env);
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&chain_id, &relayers, &1);
}

#[allow(clippy::too_many_arguments)]
fn single_inbound_sig(
    env: &Env,
    relayer_key: &SigningKey,
    contract: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> soroban_sdk::Vec<BytesN<64>> {
    let sig = sign_inbound_payload(
        env,
        relayer_key,
        contract,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(env);
    signatures.push_back(sig);
    signatures
}

fn setup_relayers(
    env: &Env,
    client: &StellarWrapContractClient,
    chain_id: u32,
) -> (SigningKey, BytesN<32>) {
    let signing_key = SigningKey::from_bytes(&[8u8; 32]);
    let relayer_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(env);
    relayers.push_back(relayer_pubkey.clone());
    client.set_bridge_relayers(&chain_id, &relayers, &1);
    (signing_key, relayer_pubkey)
}

fn sign_mint_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = construct_mint_payload(env, contract, user, period, archetype, data_hash, 1);
    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

#[allow(clippy::too_many_arguments)]
fn sign_inbound_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = construct_inbound_bridge_payload(
        env,
        contract,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
    );
    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

/// Configure `chain` with a single relayer (threshold 1) using `signer`'s
/// pubkey, then fulfill an inbound bridge for `recipient` with a valid
/// signature from that same key.
#[allow(clippy::too_many_arguments)]
fn setup_single_relayer_bridge_in(
    env: &Env,
    client: &StellarWrapContractClient<'_>,
    signer: &SigningKey,
    chain_id: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) {
    let relayer_pubkey = BytesN::from_array(env, &signer.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(env);
    relayers.push_back(relayer_pubkey);
    client.set_chain_status(&chain_id, &true);
    client.set_bridge_relayers(&chain_id, &relayers, &1);

    let sig = sign_inbound_payload(
        env,
        signer,
        &client.address,
        chain_id,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(env);
    signatures.push_back(sig);
    client.bridge_wrap_in(
        &chain_id,
        &source_nonce,
        recipient,
        &period,
        archetype,
        data_hash,
        &signatures,
    );
}

#[test]
fn test_set_and_get_bridge_relayers() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    let chain_id = 1u32;
    assert_eq!(client.get_bridge_relayers(&chain_id), None);

    let signing_key = SigningKey::from_bytes(&[8u8; 32]);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer_pubkey.clone());

    client.set_bridge_relayers(&chain_id, &relayers, &1);
    let relayer_set = client.get_bridge_relayers(&chain_id).unwrap();
    assert_eq!(relayer_set.threshold, 1);
    assert_eq!(relayer_set.relayers.len(), 1);
    assert_eq!(relayer_set.relayers.get(0).unwrap(), relayer_pubkey);
}

#[test]
fn test_set_and_check_chain_status() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    let chain_id = 137u32;
    assert!(!client.is_chain_supported(&chain_id));

    client.set_chain_status(&chain_id, &true);
    assert!(client.is_chain_supported(&chain_id));

    client.set_chain_status(&chain_eth, &true);
    assert!(client.is_chain_supported(&chain_eth));
    assert!(!client.is_chain_supported(&chain_sol));

    client.set_chain_status(&chain_eth, &false);
    assert!(!client.is_chain_supported(&chain_eth));
}

#[test]
fn test_invalid_chain_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);
    assert!(!client.is_chain_supported(&0));
}

#[test]
fn test_bridge_wrap_out_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);

    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth_address_string");
    let nonce = client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    assert_eq!(nonce, 1);

    let wrap = client.get_wrap(&user, &period).expect("wrap exists");
    assert_eq!(wrap.fsm.state, WrapState::Bridged);

    let request = client
        .get_outbound_bridge_request(&nonce)
        .expect("outbound request exists");

    assert_eq!(request.sender, user);
    assert_eq!(request.destination_chain, destination_chain);
    assert_eq!(request.nonce, 1);
    assert_eq!(request.period, period);
    assert_eq!(request.archetype, archetype);
    assert_eq!(request.data_hash, data_hash);

    let wrap = client.get_wrap(&user, &period).expect("wrap exists");
    assert_eq!(wrap.fsm.state, WrapState::Bridged);
}

#[test]
fn test_bridged_wrap_blocks_escape_routes_and_supports_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);

    let user = Address::generate(&env);
    let period = 202608u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[55u8; 32]);
    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let destination = Bytes::from_array(&env, b"destination");
    let outbound_nonce = client.bridge_wrap_out(&user, &destination_chain, &destination, &period);

    let transfer_result = catch_unwind(AssertUnwindSafe(|| {
        client.transfer_wrap(&user, &Address::generate(&env), &period);
    }));
    assert!(transfer_result.is_err());

    let burn_result = catch_unwind(AssertUnwindSafe(|| {
        client.burn_wrap(&user, &period);
    }));
    assert!(burn_result.is_err());

    let reactivate_result = catch_unwind(AssertUnwindSafe(|| {
        client.transition_wrap_state(&user, &period, &WrapState::Active);
    }));
    assert!(reactivate_result.is_err());

    let second_bridge_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &destination_chain, &destination, &period);
    }));
    assert!(second_bridge_result.is_err());
    assert_eq!(
        client.get_wrap(&user, &period).unwrap().fsm.state,
        WrapState::Bridged
    );

    client.bridge_wrap_refund(&outbound_nonce);
    assert_eq!(
        client.get_wrap(&user, &period).unwrap().fsm.state,
        WrapState::Active
    );
}

#[test]
fn test_bridge_wrap_out_disabled_chain_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;

    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth_address_string");
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);
    }));
    assert!(result.is_err());
}

#[test]
fn test_bridge_wrap_in_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    let source_chain = 1u32;
    client.set_chain_status(&source_chain, &true);

    let signing_key1 = SigningKey::from_bytes(&[1u8; 32]);
    let signing_key2 = SigningKey::from_bytes(&[2u8; 32]);
    let relayer1 = BytesN::from_array(&env, &signing_key1.verifying_key().to_bytes());
    let relayer2 = BytesN::from_array(&env, &signing_key2.verifying_key().to_bytes());

    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer1.clone());
    relayers.push_back(relayer2.clone());

    client.set_bridge_relayers(&source_chain, &relayers, &2);

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[99u8; 32]);
    let source_nonce = 101u64;

    let sig1 = sign_inbound_payload(
        &env,
        &signing_key1,
        &client.address,
        source_chain,
        source_nonce,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );
    let sig2 = sign_inbound_payload(
        &env,
        &signing_key2,
        &client.address,
        source_chain,
        source_nonce,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(sig1);
    signatures.push_back(sig2);

    assert!(!client.is_inbound_nonce_processed(&source_chain, &source_nonce));
    assert_eq!(client.balance_of(&recipient), 0);

    client.bridge_wrap_in(
        &source_chain,
        &source_nonce,
        &recipient,
        &period,
        &archetype,
        &data_hash,
        &signatures,
    );

    assert!(client.is_inbound_nonce_processed(&source_chain, &source_nonce));
    assert_eq!(client.balance_of(&recipient), 1);

    let record = client
        .get_inbound_bridge_record(&source_chain, &source_nonce)
        .expect("inbound record exists");

    assert_eq!(record.source_chain, source_chain);
    assert_eq!(record.source_nonce, source_nonce);
    assert_eq!(record.recipient, recipient);
    assert_eq!(record.period, period);
    assert_eq!(record.archetype, archetype);
    assert_eq!(record.data_hash, data_hash);

    let wrap = client.get_wrap(&recipient, &period).expect("wrap exists");
    assert_eq!(wrap.fsm.state, WrapState::Active);
}

#[test]
fn test_bridge_wrap_in_then_mint_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let source_chain = 1u32;

    let recipient = Address::generate(&env);
    let bridge_period = 202607u64;
    let bridge_archetype = symbol_short!("bridge");
    let bridge_hash = BytesN::from_array(&env, &[99u8; 32]);
    setup_single_relayer_bridge_in(
        &env,
        &client,
        &signing_key,
        source_chain,
        101u64,
        &recipient,
        bridge_period,
        &bridge_archetype,
        &bridge_hash,
        &soroban_sdk::Vec::new(&env),
    );

    let mint_period = 202608u64;
    let mint_archetype = symbol_short!("mint");
    let mint_hash = BytesN::from_array(&env, &[100u8; 32]);
    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &recipient,
        mint_period,
        &mint_archetype,
        &mint_hash,
    );
    client.mint_wrap(
        &recipient,
        &mint_period,
        &mint_archetype,
        &mint_hash,
        &1,
        &signature,
    );

    assert_eq!(
        client.get_wrap(&recipient, &mint_period).unwrap().period,
        mint_period
    );
}

#[test]
fn test_bridge_wrap_in_then_transfer_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let source_chain = 1u32;

    let recipient = Address::generate(&env);
    let destination = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[101u8; 32]);
    setup_single_relayer_bridge_in(
        &env,
        &client,
        &signing_key,
        source_chain,
        102u64,
        &recipient,
        period,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    client.set_transfer_fee(&Address::generate(&env), &Address::generate(&env), &0);
    client.transfer_wrap(&recipient, &destination, &period);

    assert!(client.get_wrap(&recipient, &period).is_none());
    assert_eq!(
        client.get_wrap(&destination, &period).unwrap().period,
        period
    );
}

#[test]
fn test_bridge_wrap_in_rejects_opted_out_recipient() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let source_chain = 1u32;

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[77u8; 32]);
    let source_nonce = 303u64;

    let sig1 = sign_inbound_payload(&env, &relayer_key, &client.address, source_chain, source_nonce, &recipient, period, &archetype, &data_hash);
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(sig1);

    client.opt_out(&recipient);
    setup_single_relayer_bridge_in(
        &env,
        &client,
        &signing_key,
        source_chain,
        source_nonce,
        &recipient,
        period,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    assert!(client.is_inbound_nonce_processed(&source_chain, &source_nonce));
    assert_eq!(client.get_wrap(&recipient, &period), None);
    assert_eq!(
        client.get_inbound_bridge_record(&source_chain, &source_nonce),
        None
    );
}

#[test]
fn test_bridge_wrap_in_rejects_terminal_states() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let source_chain = 1u32;
    let incoming_archetype = symbol_short!("bridge");
    let incoming_hash = BytesN::from_array(&env, &[77u8; 32]);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer_pubkey);
    client.set_chain_status(&source_chain, &true);
    client.set_bridge_relayers(&source_chain, &relayers, &1);

    for (source_nonce, state) in [
        (401u64, WrapState::Cancelled),
        (402u64, WrapState::Archived),
        (403u64, WrapState::Expired),
    ] {
        let recipient = Address::generate(&env);
        let period = 202607u64 + source_nonce;
        let archetype = symbol_short!("bridge");
        let data_hash = BytesN::from_array(&env, &[77u8; 32]);
        let wrap_key = DataKey::Wrap(recipient.clone(), period);
        let record = WrapRecord {
            timestamp: 100,
            data_hash: BytesN::from_array(&env, &[66u8; 32]),
            archetype: symbol_short!("existing"),
            period,
            fsm: WrapLifecycleFSM::new(state, 100),
            description: None,
            image_url: None,
        };

        env.as_contract(&client.address, || {
            env.storage().persistent().set(&wrap_key, &record);
        });

        let sig = sign_inbound_payload(
            &env,
            &signing_key,
            &client.address,
            source_chain,
            source_nonce,
            &recipient,
            period,
            &incoming_archetype,
            &incoming_hash,
        );
        let mut signatures = soroban_sdk::Vec::new(&env);
        signatures.push_back(sig);

        let result = catch_unwind(AssertUnwindSafe(|| {
            call_bridge_wrap_in(
                &env,
                &client,
                &relayer_key,
                source_chain,
                source_nonce,
                &recipient,
                period,
                &symbol_short!("bridge"),
                &BytesN::from_array(&env, &[77u8; 32]),
                &soroban_sdk::Vec::new(&env),
            );
        }));

        assert!(result.is_err());
        assert_eq!(
            client.get_wrap(&recipient, &period).unwrap().fsm.state,
            state
        );
    }
}

#[test]
fn test_bridge_wrap_in_replay_attack_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    let source_chain = 1u32;
    client.set_chain_status(&source_chain, &true);

    let signing_key1 = SigningKey::from_bytes(&[1u8; 32]);
    let relayer1 = BytesN::from_array(&env, &signing_key1.verifying_key().to_bytes());
    
    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer1.clone());
    
    client.set_bridge_relayers(&source_chain, &relayers, &1);

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[99u8; 32]);
    let source_nonce = 201u64;

    let sig1 = sign_inbound_payload(
        &env,
        &signing_key1,
        &client.address,
        source_chain,
        source_nonce,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(sig1);

    client.bridge_wrap_in(
        &source_chain,
        &source_nonce,
        &recipient,
        &period,
        &archetype,
        &data_hash,
        &signatures,
    );

    let replay_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_in(
            &source_chain,
            &source_nonce,
            &recipient,
            &period,
            &archetype,
            &data_hash,
            &signatures,
        );
    }));

    assert!(replay_result.is_err());
}

#[test]
fn test_bridge_wrap_in_threshold_signatures() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _key) = setup_test_env(&env);
    let chain_id = 1u32;
    client.set_chain_status(&chain_id, &true);

    let key1 = SigningKey::from_bytes(&[1u8; 32]);
    let key2 = SigningKey::from_bytes(&[2u8; 32]);
    let key3 = SigningKey::from_bytes(&[3u8; 32]);

    let pk1 = BytesN::from_array(&env, &key1.verifying_key().to_bytes());
    let pk2 = BytesN::from_array(&env, &key2.verifying_key().to_bytes());
    let pk3 = BytesN::from_array(&env, &key3.verifying_key().to_bytes());

    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(pk1);
    relayers.push_back(pk2);
    relayers.push_back(pk3);

    client.set_bridge_relayers(&chain_id, &relayers, &2);

    let user = Address::generate(&env);
    let period = 202601u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[10u8; 32]);

    let s1 = sign_inbound_payload(&env, &key1, &client.address, chain_id, 500, &user, period, &archetype, &data_hash);
    let s2 = sign_inbound_payload(&env, &key2, &client.address, chain_id, 500, &user, period, &archetype, &data_hash);

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    client.pause();

    let recipient_bytes = Bytes::from_array(&env, b"recipient");
    let out_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &chain_id, &recipient_bytes, &period);
    }));
    assert!(out_result.is_err());

    let sig_in = sign_inbound_payload(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        500u64,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(s1.clone());

    let in_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_in(
            &chain_id,
            &500u64,
            &user,
            &period,
            &archetype,
            &data_hash,
            &signatures,
        );
    }));
    assert!(in_result_too_few.is_err());

    signatures.push_back(s2);
    client.bridge_wrap_in(&chain_id, &500u64, &user, &period, &archetype, &data_hash, &signatures);
    assert_eq!(client.balance_of(&user), 1);
}

#[test]
fn test_mint_wrap_and_bridge_wrap_in_period_validation_parity() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let chain_id = 1u32;
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer_pubkey);
    client.set_chain_status(&chain_id, &true);
    client.set_bridge_relayers(&chain_id, &relayers, &1);

    let test_cases = [
        (202401u64, true),
        (202412u64, true),
        (205006u64, true),
        (210001u64, true),
        (210012u64, true),
        (0u64, false),
        (202312u64, false),
        (210101u64, false),
        (202400u64, false),
        (202413u64, false),
        (210000u64, false),
        (210013u64, false),
        (999999u64, false),
    ];

    for (period, is_valid) in test_cases {
        let mint_user = Address::generate(&env);
        let bridge_user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[11u8; 32]);

        let sig = sign_mint_payload(
            &env,
            &signing_key,
            &client.address,
            &mint_user,
            period,
            &archetype,
            &data_hash,
        );

        let mint_result = catch_unwind(AssertUnwindSafe(|| {
            client.mint_wrap(&mint_user, &period, &archetype, &data_hash, &1, &sig);
        }));

        let nonce = period; // unique per iteration
        let sig_in = sign_inbound_payload(
            &env,
            &signing_key,
            &client.address,
            chain_id,
            nonce,
            &bridge_user,
            period,
            &archetype,
            &data_hash,
        );
        let mut signatures = soroban_sdk::Vec::new(&env);
        signatures.push_back(sig_in);
        let bridge_result = catch_unwind(AssertUnwindSafe(|| {
            call_bridge_wrap_in(
                &env,
                &client,
                &relayer_key,
                chain_id,
                nonce,
                &bridge_user,
                period,
                &archetype,
                &data_hash,
                &soroban_sdk::Vec::new(&env),
            );
        }));

        if is_valid {
            assert!(
                mint_result.is_ok(),
                "mint_wrap should accept valid period {}",
                period
            );
            assert!(
                bridge_result.is_ok(),
                "bridge_wrap_in should accept valid period {}",
                period
            );
        } else {
            assert!(
                mint_result.is_err(),
                "mint_wrap should reject invalid period {}",
                period
            );
            assert!(
                bridge_result.is_err(),
                "bridge_wrap_in should reject invalid period {}",
                period
            );
        }
    }
}

#[test]
fn test_bridge_wrap_in_fresh_recipient_then_mint_wrap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let chain_id = 1u32;

    let recipient = Address::generate(&env);
    let period1 = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[99u8; 32]);
    let source_nonce = 101u64;

    let signatures = single_inbound_sig(
        &env,
        &relayer_key,
        &client.address,
        chain_id,
        source_nonce,
        &recipient,
        period1,
        &archetype,
        &data_hash,
    );

    // 1. Bridge wrap in for fresh recipient
    setup_single_relayer_bridge_in(
        &env,
        &client,
        &signing_key,
        chain_id,
        source_nonce,
        &recipient,
        period1,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );
    assert_eq!(client.balance_of(&recipient), 1);

    let period2 = 202608u64;
    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &recipient,
        period2,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&recipient, &period2, &archetype, &data_hash, &1, &sig);
    assert_eq!(client.balance_of(&recipient), 2);
}

#[test]
fn test_transfer_wrap_of_bridged_record_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let chain_id = 1u32;

    let from_user = Address::generate(&env);
    let to_user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[88u8; 32]);
    let source_nonce = 101u64;
    let in_sig = sign_inbound_payload(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        source_nonce,
        &from_user,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(in_sig);

    let signatures = sign_inbound_signature(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        101u64,
        &from_user,
        period,
        &archetype,
        &data_hash,
    );
    client.bridge_wrap_in(
        &chain_id,
        &101u64,
        &from_user,
        &period,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    assert_eq!(client.balance_of(&from_user), 1);
    assert_eq!(client.balance_of(&to_user), 0);

    client.transfer_wrap(&from_user, &to_user, &period);

    assert_eq!(client.balance_of(&from_user), 0);
    assert_eq!(client.balance_of(&to_user), 1);
    let wrap = client
        .get_wrap(&to_user, &period)
        .expect("transferred wrap exists");
    assert_eq!(wrap.fsm.state, WrapState::Active);
}

#[test]
fn test_bridge_wrap_in_index_invariants() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let chain_id = 1u32;

    let recipient = Address::generate(&env);
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[77u8; 32]);

    let periods = [202601u64, 202602u64, 202603u64];

    for (idx, &period) in periods.iter().enumerate() {
        let nonce = (idx + 1) as u64;
        let in_sig = sign_inbound_payload(
            &env,
            &signing_key,
            &client.address,
            chain_id,
            nonce,
            &recipient,
            period,
            &archetype,
            &data_hash,
        );
        let mut signatures = soroban_sdk::Vec::new(&env);
        signatures.push_back(in_sig);
        client.bridge_wrap_in(
            &chain_id,
            &nonce,
            &recipient,
            &period,
            &archetype,
            &data_hash,
            &soroban_sdk::Vec::new(&env),
        );

        let count = client.balance_of(&recipient) as u32;
        assert_eq!(count, (idx + 1) as u32);

        let (wrap_periods_len, user_periods_len) = env.as_contract(&client.address, || {
            let wrap_periods: soroban_sdk::Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::WrapPeriods(recipient.clone()))
                .unwrap();
            let user_periods: soroban_sdk::Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::UserPeriods(recipient.clone()))
                .unwrap();
            (wrap_periods.len(), user_periods.len())
        });

        assert_eq!(count, wrap_periods_len);
        assert_eq!(wrap_periods_len, user_periods_len);
    }
}

#[test]
fn test_bridge_wrap_in_existing_period_updates_rather_than_duplicating() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let mut relayers = soroban_sdk::Vec::new(&env);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&1u32, &relayers, &1);
    let chain_id = 1u32;

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[66u8; 32]);

    // First bridge in: creates new wrap record
    let in_sig_1 = sign_inbound_payload(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        1u64,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures_1 = soroban_sdk::Vec::new(&env);
    signatures_1.push_back(in_sig_1);
    client.bridge_wrap_in(
        &chain_id,
        &1u64,
        &recipient,
        &period,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    // Bridge wrap out: sets state to Bridged
    let recipient_bytes = Bytes::from_array(&env, b"eth_address");
    client.bridge_wrap_out(&recipient, &chain_id, &recipient_bytes, &period);

    let wrap_bridged = client.get_wrap(&recipient, &period).expect("wrap exists");
    assert_eq!(wrap_bridged.fsm.state, WrapState::Bridged);

    // Second bridge in (e.g. returned/re-bridged from another chain): updates existing record
    let in_sig_2 = sign_inbound_payload(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        2u64,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures_2 = soroban_sdk::Vec::new(&env);
    signatures_2.push_back(in_sig_2);
    client.bridge_wrap_in(
        &chain_id,
        &2u64,
        &recipient,
        &period,
        &archetype,
        &data_hash,
        &soroban_sdk::Vec::new(&env),
    );

    let wrap_active = client.get_wrap(&recipient, &period).expect("wrap exists");
    assert_eq!(wrap_active.fsm.state, WrapState::Active);

    let count = client.balance_of(&recipient) as u32;
    assert_eq!(count, 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #51)")]
fn test_bridge_wrap_in_legacy_index_invariant_guard() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[99u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let chain_id = 1u32;
    let relayer_key = setup_default_relayer(&env, &client, chain_id);
    let recipient = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let period = 202607u64;

    // First bridge in succeeds
    call_bridge_wrap_in(
        &env,
        &client,
        &relayer_key,
        chain_id,
        1u64,
        &recipient,
        period,
        &archetype,
        &data_hash,
    );

    // Simulate legacy state: user has count > 0 but WrapPeriods is missing
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(recipient.clone()));
    });

    // Subsequent bridge-in for fresh period must panic with StorageInvariantViolation (error #56)
    let period2 = 202608u64;
    let data_hash2 = BytesN::from_array(&env, &[2u8; 32]);
    call_bridge_wrap_in(
        &env,
        &client,
        &relayer_key,
        chain_id,
        2u64,
        &recipient,
        period2,
        &archetype,
        &data_hash2,
    );
}

#[test]
fn test_bridge_wrap_out_rejects_transfer_wrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth");
    client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    let recipient2 = Address::generate(&env);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.transfer_wrap(&user, &recipient2, &period);
    }));
    assert!(result.is_err());
}

#[test]
fn test_bridge_wrap_out_rejects_burn_wrap() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth");
    client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.burn_wrap(&user, &period);
    }));
    assert!(result.is_err());
}

#[test]
fn test_bridge_wrap_out_rejects_second_bridge_wrap_out() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth");
    client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);
    }));
    assert!(result.is_err());
}

#[test]
fn test_bridge_wrap_out_rejects_transition_to_active() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth");
    client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.transition_wrap_state(&user, &period, &WrapState::Active);
    }));
    assert!(result.is_err());
}

#[test]
fn test_get_outbound_bridge_request_fields() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[12u8; 32]);

    let signature = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &signature);

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"recipient_eth_address_string");
    let nonce = client.bridge_wrap_out(&user, &destination_chain, &recipient_bytes, &period);

    let request = client
        .get_outbound_bridge_request(&nonce)
        .expect("outbound request exists");

    assert_eq!(request.nonce, nonce);
    assert_eq!(request.sender, user);
    assert_eq!(request.destination_chain, destination_chain);
    assert_eq!(request.recipient_address, recipient_bytes);
    assert_eq!(request.period, period);
    assert_eq!(request.archetype, archetype);
    assert_eq!(request.data_hash, data_hash);
    assert!(request.timestamp > 0);
}
