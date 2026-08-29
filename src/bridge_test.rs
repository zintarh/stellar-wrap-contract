#![cfg(test)]

extern crate std;

use super::*;
use crate::signature::construct_mint_payload;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup_test_env<'a>(
    env: &'a Env,
) -> (StellarWrapContractClient<'a>, Address, Address, SigningKey) {
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let relayer = Address::generate(env);

    client.initialize(&admin, &admin_pubkey);
    (client, admin, relayer, signing_key)
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

#[test]
fn test_set_and_get_bridge_relayer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, _key) = setup_test_env(&env);

    assert_eq!(client.get_bridge_relayer(), None);

    client.set_bridge_relayer(&relayer);
    assert_eq!(client.get_bridge_relayer(), Some(relayer));
}

#[test]
fn test_set_and_check_chain_status() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, _key) = setup_test_env(&env);

    let chain_eth = 1u32;
    let chain_sol = 900u32;

    assert!(!client.is_chain_supported(&chain_eth));
    assert!(!client.is_chain_supported(&chain_sol));

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

    let (client, _admin, _relayer, _key) = setup_test_env(&env);
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
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32; // Polygon
    client.set_chain_status(&dest_chain, &true);

    let recipient = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    assert_eq!(client.get_outbound_nonce(), 0);

    let nonce = client.bridge_wrap_out(&user, &dest_chain, &recipient, &period);

    assert_eq!(nonce, 1);
    assert_eq!(client.get_outbound_nonce(), 1);

    let request = client
        .get_outbound_bridge_request(&nonce)
        .expect("request exists");
    assert_eq!(request.nonce, 1);
    assert_eq!(request.sender, user);
    assert_eq!(request.destination_chain, dest_chain);
    assert_eq!(request.recipient_address, recipient);
    assert_eq!(request.period, period);
    assert_eq!(request.archetype, archetype);
    assert_eq!(request.data_hash, data_hash);

    let wrap = client.get_wrap(&user, &period).expect("wrap exists");
    assert_eq!(wrap.fsm.state, WrapState::Pending);
}

#[test]
fn test_bridge_wrap_out_disabled_chain_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let recipient = Bytes::from_array(&env, b"recipient");

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &999u32, &recipient, &period);
    }));

    assert!(result.is_err());
}

#[test]
fn test_bridge_wrap_in_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, _key) = setup_test_env(&env);

    client.set_bridge_relayer(&relayer);

    let source_chain = 1u32;
    client.set_chain_status(&source_chain, &true);

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[99u8; 32]);
    let source_nonce = 101u64;

    assert!(!client.is_inbound_nonce_processed(&source_chain, &source_nonce));
    assert_eq!(client.balance_of(&recipient), 0);

    client.bridge_wrap_in(
        &source_chain,
        &source_nonce,
        &recipient,
        &period,
        &archetype,
        &data_hash,
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
fn test_bridge_wrap_in_replay_attack_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, _key) = setup_test_env(&env);

    client.set_bridge_relayer(&relayer);
    let source_chain = 1u32;
    client.set_chain_status(&source_chain, &true);

    let recipient = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("bridge");
    let data_hash = BytesN::from_array(&env, &[88u8; 32]);
    let source_nonce = 202u64;

    client.bridge_wrap_in(
        &source_chain,
        &source_nonce,
        &recipient,
        &period,
        &archetype,
        &data_hash,
    );

    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_in(
            &source_chain,
            &source_nonce,
            &recipient,
            &period,
            &archetype,
            &data_hash,
        );
    }));

    assert!(result.is_err());
}

#[test]
fn test_bridge_paused_blocks_operations() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, signing_key) = setup_test_env(&env);

    client.set_bridge_relayer(&relayer);
    let chain_id = 1u32;
    client.set_chain_status(&chain_id, &true);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    client.pause();

    let recipient_bytes = Bytes::from_array(&env, b"recipient");
    let out_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &chain_id, &recipient_bytes, &period);
    }));
    assert!(out_result.is_err());

    let in_result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_in(&chain_id, &500u64, &user, &period, &archetype, &data_hash);
    }));
    assert!(in_result.is_err());
}

#[test]
fn test_bridged_out_record_transfer_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let recipient_user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32;
    client.set_chain_status(&dest_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);

    let res = catch_unwind(AssertUnwindSafe(|| {
        client.transfer_wrap(&user, &recipient_user, &period);
    }));
    assert!(res.is_err());
}

#[test]
fn test_bridged_out_record_burn_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32;
    client.set_chain_status(&dest_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);

    let res = catch_unwind(AssertUnwindSafe(|| {
        client.burn_wrap(&user, &period);
    }));
    assert!(res.is_err());
}

#[test]
fn test_bridged_out_record_second_bridge_out_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32;
    client.set_chain_status(&dest_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);

    let res = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);
    }));
    assert!(res.is_err());
}

#[test]
fn test_bridged_out_record_transition_to_active_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32;
    client.set_chain_status(&dest_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);

    let res = catch_unwind(AssertUnwindSafe(|| {
        client.transition_wrap_state(&user, &period, &WrapState::Active);
    }));
    assert!(res.is_err());
}

#[test]
fn test_bridged_out_record_all_acceptance_criteria() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let to_user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    let dest_chain = 137u32;
    client.set_chain_status(&dest_chain, &true);
    let recipient_bytes = Bytes::from_array(&env, b"0x1234567890abcdef1234567890abcdef12345678");

    let nonce = client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);

    // 1. get_outbound_bridge_request(nonce) returns expected fields
    let req = client
        .get_outbound_bridge_request(&nonce)
        .expect("request exists");
    assert_eq!(req.nonce, nonce);
    assert_eq!(req.sender, user);
    assert_eq!(req.destination_chain, dest_chain);
    assert_eq!(req.recipient_address, recipient_bytes);
    assert_eq!(req.period, period);
    assert_eq!(req.archetype, archetype);
    assert_eq!(req.data_hash, data_hash);

    // 2. transfer_wrap of the same period is rejected
    assert!(catch_unwind(AssertUnwindSafe(|| {
        client.transfer_wrap(&user, &to_user, &period);
    }))
    .is_err());

    // 3. burn_wrap of the same period is rejected
    assert!(catch_unwind(AssertUnwindSafe(|| {
        client.burn_wrap(&user, &period);
    }))
    .is_err());

    // 4. A second bridge_wrap_out for the same period is rejected
    assert!(catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &dest_chain, &recipient_bytes, &period);
    }))
    .is_err());

    // 5. transition_wrap_state(user, period, Active) by the owner is rejected
    assert!(catch_unwind(AssertUnwindSafe(|| {
        client.transition_wrap_state(&user, &period, &WrapState::Active);
    }))
    .is_err());
}
