#![cfg(test)]
extern crate std;

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, TryIntoVal,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::storage_types::{DataKey, WrapRecord};

fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let mut payload = Bytes::new(env);
    payload.append(&contract.to_xdr(env));
    payload.append(&user.clone().to_xdr(env));
    payload.append(&period.to_xdr(env));
    payload.append(&archetype.clone().to_xdr(env));
    payload.append(&data_hash.clone().to_xdr(env));

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

#[test]
fn test_minting_flow() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &dummy_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &dummy_hash, &signature);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, dummy_hash);
}

#[test]
fn test_mint_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[2u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2024u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &signature);

    let events = env.events().all();
    let last_event = events.last().expect("No events found");
    let (_, topics, data) = last_event;

    // Convert Vals to concrete types for comparison
    let event_topic: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let event_user: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_period: u64 = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let event_archetype: Symbol = data.try_into_val(&env).unwrap();

    assert_eq!(event_topic, symbol_short!("mint"));
    assert_eq!(event_user, user);
    assert_eq!(event_period, period);
    assert_eq!(event_archetype, archetype);
}

#[test]
fn test_balance_of_and_count() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[3u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("soroban");
    let hash = BytesN::from_array(&env, &[1u8; 32]);

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2021,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &2021, &archetype, &hash, &sig1);

    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2022,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &2022, &archetype, &hash, &sig2);

    assert_eq!(client.balance_of(&user), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    client.initialize(&admin, &pubkey);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[4u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &sig);
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);
}

#[test]
fn test_update_admin_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.update_admin(&new_admin);
    assert_eq!(client.get_admin().unwrap(), new_admin);
}

#[test]
fn test_token_metadata() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    assert_eq!(client.decimals(), 0);
    assert_eq!(
        client.name(),
        String::from_str(&env, "Stellar Wrap Registry")
    );
    assert_eq!(client.symbol(), String::from_str(&env, "WRAP"));
}

// ─── Issue #56: contract_info tests ─────────────────────────────────────────

#[test]
fn test_contract_info_returns_correct_fields() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let info = client.contract_info();
    assert_eq!(info.name, String::from_str(&env, "Stellar Wrap Registry"));
    assert_eq!(info.version, String::from_str(&env, "0.1.0"));
    assert_eq!(
        info.repo,
        String::from_str(&env, "https://github.com/zintarh/stellar-wrap-contract")
    );
    assert_eq!(
        info.description,
        String::from_str(&env, "Soulbound token registry for Stellar Wrap")
    );
}

// ─── Issue #84: extend_ttl tests ────────────────────────────────────────────

#[test]
fn test_extend_ttl_existing_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202512u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);

    // Anyone can call extend_ttl — no auth required
    client.extend_ttl(&user, &period);

    // Record should still be readable after extending TTL
    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, hash);
}

#[test]
fn test_extend_ttl_nonexistent_wrap_does_not_panic() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let user = Address::generate(&env);
    // Should not panic even if no wrap exists for this user/period
    client.extend_ttl(&user, &9999);
}

// ─── Issue #81: concurrent mints for different users same period ────────────

#[test]
fn test_concurrent_mints_different_users_same_period() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[10u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("arch");
    let hash_a = BytesN::from_array(&env, &[10u8; 32]);
    let hash_b = BytesN::from_array(&env, &[20u8; 32]);

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &hash_a,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period,
        &archetype,
        &hash_b,
    );

    // Both mints for the same period should succeed
    client.mint_wrap(&user_a, &period, &archetype, &hash_a, &sig_a);
    client.mint_wrap(&user_b, &period, &archetype, &hash_b, &sig_b);

    // Records are independent
    let wrap_a = client.get_wrap(&user_a, &period).unwrap();
    let wrap_b = client.get_wrap(&user_b, &period).unwrap();
    assert_eq!(wrap_a.data_hash, hash_a);
    assert_eq!(wrap_b.data_hash, hash_b);
    assert_ne!(wrap_a.data_hash, wrap_b.data_hash);

    // Individual balances are correct
    assert_eq!(client.balance_of(&user_a), 1);
    assert_eq!(client.balance_of(&user_b), 1);

    // Each user's record doesn't affect the other
    assert!(client.get_wrap(&user_a, &period).is_some());
    assert!(client.get_wrap(&user_b, &period).is_some());
}

// ─── Issue #75: structured event verification ──────────────────────────────

#[test]
fn test_mint_event_structured_matching() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[11u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);

    // Event schema: topics = (Symbol("mint"), Address, u64), data = Symbol
    let events = env.events().all();
    let last_event = events.last().expect("Expected at least one event");
    let (event_contract, topics, data) = last_event;

    // Verify event is emitted by the correct contract
    assert_eq!(event_contract, contract_id);

    // Verify topic count — mint events must have exactly 3 topics
    assert_eq!(topics.len(), 3, "Mint event must have exactly 3 topics");

    // Verify each topic by type and value
    let topic_0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let topic_1: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let topic_2: u64 = topics.get(2).unwrap().try_into_val(&env).unwrap();

    assert_eq!(
        topic_0,
        symbol_short!("mint"),
        "Topic 0 must be 'mint' Symbol"
    );
    assert_eq!(topic_1, user, "Topic 1 must be the user Address");
    assert_eq!(topic_2, period, "Topic 2 must be the period u64");

    // Verify data is the archetype Symbol
    let event_data: Symbol = data.try_into_val(&env).unwrap();
    assert_eq!(
        event_data, archetype,
        "Event data must be the archetype Symbol"
    );
}

#[test]
fn test_mint_events_multiple_users_correct_schema() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[12u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype_a = symbol_short!("builder");
    let archetype_b = symbol_short!("defi");
    let hash_a = BytesN::from_array(&env, &[10u8; 32]);
    let hash_b = BytesN::from_array(&env, &[20u8; 32]);
    let period_a = 202501u64;
    let period_b = 202502u64;

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period_a,
        &archetype_a,
        &hash_a,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period_b,
        &archetype_b,
        &hash_b,
    );

    client.mint_wrap(&user_a, &period_a, &archetype_a, &hash_a, &sig_a);
    client.mint_wrap(&user_b, &period_b, &archetype_b, &hash_b, &sig_b);

    let events = env.events().all();

    // Collect mint events emitted by our contract
    let mut mint_events = soroban_sdk::vec![&env];
    for event in events.iter() {
        let (addr, topics, _data) = &event;
        if *addr == contract_id && topics.len() == 3 {
            let t: Result<Symbol, _> = topics.get(0).unwrap().try_into_val(&env);
            if t.map_or(false, |s| s == symbol_short!("mint")) {
                mint_events.push_back(event.clone());
            }
        }
    }

    assert_eq!(mint_events.len(), 2, "Expected exactly 2 mint events");

    // Verify first mint event (user_a)
    let (_, topics_a, data_a) = mint_events.get(0).unwrap();
    let ev_user_a: Address = topics_a.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_period_a: u64 = topics_a.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_arch_a: Symbol = data_a.try_into_val(&env).unwrap();
    assert_eq!(ev_user_a, user_a);
    assert_eq!(ev_period_a, period_a);
    assert_eq!(ev_arch_a, archetype_a);

    // Verify second mint event (user_b)
    let (_, topics_b, data_b) = mint_events.get(1).unwrap();
    let ev_user_b: Address = topics_b.get(1).unwrap().try_into_val(&env).unwrap();
    let ev_period_b: u64 = topics_b.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_arch_b: Symbol = data_b.try_into_val(&env).unwrap();
    assert_eq!(ev_user_b, user_b);
    assert_eq!(ev_period_b, period_b);
    assert_eq!(ev_arch_b, archetype_b);
}

// ─── Issue #80: verify_data tests ───────────────────────────────────────────

#[test]
fn test_verify_data_matching_hash() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[5u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let data_json = Bytes::from_slice(&env, b"{\"score\":100,\"level\":\"gold\"}");
    let data_hash_raw = env.crypto().sha256(&data_json);
    let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    assert!(client.verify_data(&user, &period, &data_json));
}

#[test]
fn test_verify_data_non_matching_hash() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[6u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let original_data = Bytes::from_slice(&env, b"{\"score\":100}");
    let data_hash_raw = env.crypto().sha256(&original_data);
    let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    let tampered_data = Bytes::from_slice(&env, b"{\"score\":999}");
    assert!(!client.verify_data(&user, &period, &tampered_data));
}

#[test]
fn test_verify_data_no_wrap_exists() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let user = Address::generate(&env);
    let data = Bytes::from_slice(&env, b"anything");
    assert!(!client.verify_data(&user, &9999, &data));
}

// ─── Issue #87: get_latest_wrap tests ───────────────────────────────────────

#[test]
fn test_get_latest_wrap_returns_most_recent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[10u8; 32]);
    let hash2 = BytesN::from_array(&env, &[20u8; 32]);
    let hash3 = BytesN::from_array(&env, &[30u8; 32]);

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2022,
        &archetype,
        &hash1,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2024,
        &archetype,
        &hash2,
    );
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2023,
        &archetype,
        &hash3,
    );

    client.mint_wrap(&user, &2022, &archetype, &hash1, &sig1);
    client.mint_wrap(&user, &2024, &archetype, &hash2, &sig2);
    client.mint_wrap(&user, &2023, &archetype, &hash3, &sig3);

    let latest = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest.period, 2024);
    assert_eq!(latest.data_hash, hash2);
}

#[test]
fn test_get_latest_wrap_no_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let user = Address::generate(&env);
    assert!(client.get_latest_wrap(&user).is_none());
}

#[test]
fn test_get_latest_wrap_single_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[8u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[55u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 2025u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);

    let latest = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest.period, 2025);
    assert_eq!(latest.data_hash, hash);
}

// ─── Issue #85: negative tests before initialize ────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_mint_wrap_before_init_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    env.mock_all_auths();

    let user = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let archetype = symbol_short!("arch");
    let sig = BytesN::from_array(&env, &[0u8; 64]);

    client.mint_wrap(&user, &2024, &archetype, &hash, &sig);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_update_admin_before_init_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    env.mock_all_auths();

    let new_admin = Address::generate(&env);
    client.update_admin(&new_admin);
}

#[test]
fn test_get_admin_before_init_returns_none() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    assert!(client.get_admin().is_none());
}

// ─── Issue #27: revoke_wrap tests ─────────────────────────────────────────

#[test]
fn test_revoke_wrap_flow_event_and_remint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[13u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype = symbol_short!("arch");
    let hash_1 = BytesN::from_array(&env, &[31u8; 32]);
    let hash_2 = BytesN::from_array(&env, &[32u8; 32]);

    let sig_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash_1,
    );
    client.mint_wrap(&user, &period, &archetype, &hash_1, &sig_1);
    assert_eq!(client.balance_of(&user), 1);

    client.revoke_wrap(&user, &period);

    assert!(client.get_wrap(&user, &period).is_none());
    assert_eq!(client.balance_of(&user), 0);

    let events = env.events().all();
    let last_event = events.last().expect("Expected revoke event");
    let (_, topics, data) = last_event;

    let event_topic: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let event_user: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_period: u64 = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let revoked: bool = data.try_into_val(&env).unwrap();

    assert_eq!(event_topic, symbol_short!("revoke"));
    assert_eq!(event_user, user);
    assert_eq!(event_period, period);
    assert!(revoked);

    // Re-mint the same period should now succeed after revoke.
    let sig_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash_2,
    );
    client.mint_wrap(&user, &period, &archetype, &hash_2, &sig_2);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, hash_2);
    assert_eq!(client.balance_of(&user), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_revoke_missing_wrap_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[14u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    client.revoke_wrap(&user, &2026);
}

#[test]
#[should_panic]
fn test_revoke_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[15u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    // Seed one wrap record directly to isolate auth behavior on revoke.
    env.as_contract(&contract_id, || {
        let wrap_key = DataKey::Wrap(user.clone(), 2026);
        let count_key = DataKey::WrapCount(user.clone());
        let record = WrapRecord {
            timestamp: env.ledger().timestamp(),
            data_hash: BytesN::from_array(&env, &[16u8; 32]),
            archetype: symbol_short!("arch"),
            period: 2026,
            image_uri: String::from_str(&env, ""),
        };
        env.storage().persistent().set(&wrap_key, &record);
        env.storage().persistent().set(&count_key, &1u32);
    });

    // No auth mocking: admin.require_auth() must fail.
    client.revoke_wrap(&user, &2026);
}

// ─── Issue #82: temporary mint guard tests ─────────────────────────────────

#[test]
fn test_mint_guard_uses_temporary_storage_and_clears_on_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[13u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[13u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    let guard_key = DataKey::MintGuard(user.clone());
    env.as_contract(&contract_id, || {
        assert!(!env.storage().temporary().has(&guard_key));
        assert!(!env.storage().persistent().has(&guard_key));
    });
}

#[test]
fn test_mint_guard_on_failure_leaves_no_residual_state() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[14u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[14u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    // First mint succeeds.
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    // Second mint panics (duplicate).
    let duplicate = catch_unwind(AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period, &archetype, &data_hash, &signature)
    }));
    assert!(duplicate.is_err());

    let guard_key = DataKey::MintGuard(user.clone());
    env.as_contract(&contract_id, || {
        // Failed invocations revert, so no leftover guard entry remains.
        assert!(!env.storage().temporary().has(&guard_key));
        assert!(!env.storage().persistent().has(&guard_key));
    });
}

// ─── Issue #39: update_admin event emission test ────────────────────────────

#[test]
fn test_update_admin_emits_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.update_admin(&new_admin);

    let events = env.events().all();
    let last_event = events.last().expect("Expected at least one event");
    let (_, topics, data) = last_event;

    let topic_0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let topic_1: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_0, symbol_short!("admin"));
    assert_eq!(topic_1, symbol_short!("updated"));

    // data is (old_admin, new_admin)
    let (old_admin_val, new_admin_val): (Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(old_admin_val, admin);
    assert_eq!(new_admin_val, new_admin);
}

// ─── Issue #34: update_admin authorization failure tests ────────────────────

#[test]
#[should_panic]
fn test_update_admin_unauthorized_fails() {
    // No mock_all_auths — auth requirement is not satisfied, should panic
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    client.update_admin(&new_admin);
}

#[test]
#[should_panic]
fn test_update_admin_by_non_admin_fails() {
    // A different address tries to call update_admin — should panic
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);

    // Only mock auth for non_admin — current_admin.require_auth() will fail
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_admin",
            args: (&new_admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.update_admin(&new_admin);
}

// ─── Issue #55: zero-hash and edge-case hash tests ──────────────────────────

#[test]
#[should_panic]
fn test_mint_wrap_zero_hash_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[20u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &zero_hash);
    // Must panic with InvalidDataHash
    client.mint_wrap(&user, &period, &archetype, &zero_hash, &sig);
}

#[test]
fn test_mint_wrap_non_zero_hash_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[21u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    // A hash with only the last byte set — not all-zero, should succeed
    let mut hash_bytes = [0u8; 32];
    hash_bytes[31] = 1;
    let edge_hash = BytesN::from_array(&env, &hash_bytes);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &edge_hash);
    client.mint_wrap(&user, &period, &archetype, &edge_hash, &sig);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, edge_hash);
}

#[test]
fn test_mint_wrap_max_hash_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[22u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let max_hash = BytesN::from_array(&env, &[0xff; 32]);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &max_hash);
    client.mint_wrap(&user, &period, &archetype, &max_hash, &sig);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, max_hash);
}

// ─── Issue #30: upgrade authorization test ──────────────────────────────────

#[test]
#[should_panic]
fn test_upgrade_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    // No auth mocked — must panic because admin did not authorize
    let fake_wasm = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade(&fake_wasm);
}

// ─── Issue #59: update_wrap tests ──────────────────────────────────────────

fn sign_update_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    new_archetype: &Symbol,
    new_data_hash: &BytesN<32>,
) -> BytesN<64> {
    sign_payload(env, signer, contract, user, period, new_archetype, new_data_hash)
}

#[test]
fn test_update_wrap_succeeds_and_preserves_timestamp() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[30u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);

    let sig1 = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash1);
    client.mint_wrap(&user, &period, &archetype, &hash1, &sig1);

    let before = client.get_wrap(&user, &period).unwrap();

    let new_hash = BytesN::from_array(&env, &[99u8; 32]);
    let new_arch = symbol_short!("builder");
    let sig2 = sign_update_payload(&env, &signing_key, &contract_id, &user, period, &new_arch, &new_hash);
    client.update_wrap(&user, &period, &new_hash, &new_arch, &sig2);

    let after = client.get_wrap(&user, &period).unwrap();
    assert_eq!(after.timestamp, before.timestamp, "Original timestamp must be preserved");
    assert_eq!(after.data_hash, new_hash);
    assert_eq!(after.archetype, new_arch);
    assert_eq!(after.period, period);
}

#[test]
fn test_update_wrap_emits_update_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[31u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);
    let sig1 = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash1);
    client.mint_wrap(&user, &period, &archetype, &hash1, &sig1);

    let new_hash = BytesN::from_array(&env, &[98u8; 32]);
    let new_arch = symbol_short!("defi");
    let sig2 = sign_update_payload(&env, &signing_key, &contract_id, &user, period, &new_arch, &new_hash);
    client.update_wrap(&user, &period, &new_hash, &new_arch, &sig2);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_, topics, data) = last_event;

    let topic_0: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let topic_1: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let topic_2: u64 = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_arch: Symbol = data.try_into_val(&env).unwrap();

    assert_eq!(topic_0, symbol_short!("update"));
    assert_eq!(topic_1, user);
    assert_eq!(topic_2, period);
    assert_eq!(ev_arch, new_arch);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_update_wrap_nonexistent_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[32u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let new_hash = BytesN::from_array(&env, &[99u8; 32]);
    let new_arch = symbol_short!("arch");
    let sig = sign_update_payload(&env, &signing_key, &contract_id, &user, 9999, &new_arch, &new_hash);
    client.update_wrap(&user, &9999, &new_hash, &new_arch, &sig);
}

#[test]
#[should_panic]
fn test_update_wrap_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[33u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);
    let sig1 = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash1);
    client.mint_wrap(&user, &period, &archetype, &hash1, &sig1);

    // Reset auth — no admin auth mocked
    let env2 = Env::default();
    let contract_id2 = env2.register_contract(None, StellarWrapContract);
    let client2 = StellarWrapContractClient::new(&env2, &contract_id2);
    client2.initialize(&admin, &admin_pubkey);

    let new_hash = BytesN::from_array(&env2, &[99u8; 32]);
    let new_arch = symbol_short!("arch");
    let sig2 = sign_update_payload(&env2, &signing_key, &contract_id2, &user, period, &new_arch, &new_hash);
    // No auth mocked — must panic
    client2.update_wrap(&user, &period, &new_hash, &new_arch, &sig2);
}

#[test]
#[should_panic]
fn test_update_wrap_zero_hash_rejected() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[34u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);
    let sig1 = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash1);
    client.mint_wrap(&user, &period, &archetype, &hash1, &sig1);

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let sig2 = sign_update_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &zero_hash);
    client.update_wrap(&user, &period, &zero_hash, &archetype, &sig2);
}

// ─── Merkle batch claim tests ───────────────────────────────────────────────

use crate::merkle::{compute_merkle_leaf, hash_pair};

fn merkle_root_for_leaves(env: &Env, leaves: &[BytesN<32>]) -> BytesN<32> {
    assert!(!leaves.is_empty());
    if leaves.len() == 1 {
        return leaves[0].clone();
    }
    let mut layer: soroban_sdk::Vec<BytesN<32>> = soroban_sdk::Vec::new(env);
    for leaf in leaves {
        layer.push_back(leaf.clone());
    }
    while layer.len() > 1 {
        let mut next = soroban_sdk::Vec::new(env);
        let mut i = 0u32;
        while i < layer.len() {
            if i + 1 < layer.len() {
                let pair = hash_pair(env, &layer.get(i).unwrap(), &layer.get(i + 1).unwrap());
                next.push_back(pair);
                i += 2;
            } else {
                next.push_back(layer.get(i).unwrap());
                i += 1;
            }
        }
        layer = next;
    }
    layer.get(0).unwrap()
}

fn merkle_proof_for_index(env: &Env, leaves: &[BytesN<32>], index: usize) -> soroban_sdk::Vec<BytesN<32>> {
    let mut proof = soroban_sdk::Vec::new(env);
    let mut idx = index;
    let mut layer: std::vec::Vec<BytesN<32>> = leaves.to_vec();
    while layer.len() > 1 {
        let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        if sibling_idx < layer.len() {
            proof.push_back(layer[sibling_idx].clone());
        }
        let mut next = std::vec::Vec::new();
        let mut i = 0;
        while i < layer.len() {
            if i + 1 < layer.len() {
                next.push(hash_pair(env, &layer[i], &layer[i + 1]));
                i += 2;
            } else {
                next.push(layer[i].clone());
                i += 1;
            }
        }
        idx /= 2;
        layer = next;
    }
    proof
}

#[test]
fn test_set_merkle_root_and_valid_claim() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[40u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("builder");
    let data_hash = BytesN::from_array(&env, &[50u8; 32]);

    let leaf = compute_merkle_leaf(&env, &user, period, &archetype, &data_hash);
    let root = merkle_root_for_leaves(&env, &[leaf.clone()]);
    client.set_merkle_root(&period, &root);

    let proof = merkle_proof_for_index(&env, &[leaf], 0);
    client.claim_wrap(&user, &period, &archetype, &data_hash, &proof);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, data_hash);
    assert_eq!(wrap.archetype, archetype);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_merkle_invalid_proof_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("builder");
    let data_hash = BytesN::from_array(&env, &[51u8; 32]);
    let root = BytesN::from_array(&env, &[99u8; 32]);
    client.set_merkle_root(&period, &root);

    let bad_proof = soroban_sdk::vec![&env, BytesN::from_array(&env, &[1u8; 32])];
    client.claim_wrap(&user, &period, &archetype, &data_hash, &bad_proof);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_merkle_double_claim_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("builder");
    let data_hash = BytesN::from_array(&env, &[52u8; 32]);
    let leaf = compute_merkle_leaf(&env, &user, period, &archetype, &data_hash);
    let root = merkle_root_for_leaves(&env, &[leaf.clone()]);
    client.set_merkle_root(&period, &root);
    let proof = merkle_proof_for_index(&env, &[leaf], 0);

    client.claim_wrap(&user, &period, &archetype, &data_hash, &proof);
    client.claim_wrap(&user, &period, &archetype, &data_hash, &proof);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_merkle_wrong_user_proof_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 202512u64;
    let archetype = symbol_short!("builder");
    let hash_a = BytesN::from_array(&env, &[53u8; 32]);
    let hash_b = BytesN::from_array(&env, &[54u8; 32]);

    let leaf_a = compute_merkle_leaf(&env, &user_a, period, &archetype, &hash_a);
    let leaf_b = compute_merkle_leaf(&env, &user_b, period, &archetype, &hash_b);
    let root = merkle_root_for_leaves(&env, &[leaf_a.clone(), leaf_b.clone()]);
    client.set_merkle_root(&period, &root);

    let proof_for_a = merkle_proof_for_index(&env, &[leaf_a, leaf_b], 0);
    client.claim_wrap(&user_b, &period, &archetype, &hash_b, &proof_for_a);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_merkle_wrong_period_proof_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("builder");
    let data_hash = BytesN::from_array(&env, &[55u8; 32]);
    let leaf_2024 = compute_merkle_leaf(&env, &user, 2024, &archetype, &data_hash);
    let root = merkle_root_for_leaves(&env, &[leaf_2024.clone()]);
    client.set_merkle_root(&2025, &root);

    let proof = merkle_proof_for_index(&env, &[leaf_2024], 0);
    client.claim_wrap(&user, &2025, &archetype, &data_hash, &proof);
}

// ─── Schema migration tests ─────────────────────────────────────────────────

use crate::storage_types::{WrapRecordV1, SCHEMA_VERSION, SCHEMA_VERSION_V2};

#[test]
fn test_lazy_migration_v1_record_readable() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    let period = 2024u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[70u8; 32]);

    env.as_contract(&contract_id, || {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let v1 = WrapRecordV1 {
            timestamp: 1000,
            data_hash: data_hash.clone(),
            archetype: archetype.clone(),
            period,
        };
        env.storage().persistent().set(&wrap_key, &v1);
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &SCHEMA_VERSION);
    });

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, data_hash);
    assert_eq!(wrap.image_uri, String::from_str(&env, ""));
}

#[test]
fn test_migrate_advances_schema_version() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    assert_eq!(client.get_schema_version(), SCHEMA_VERSION);
    let new_version = client.migrate(&SCHEMA_VERSION, &SCHEMA_VERSION_V2);
    assert_eq!(new_version, SCHEMA_VERSION_V2);
    assert_eq!(client.get_schema_version(), SCHEMA_VERSION_V2);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_migrate_only_once_per_transition() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    client.migrate(&SCHEMA_VERSION, &SCHEMA_VERSION_V2);
    client.migrate(&SCHEMA_VERSION, &SCHEMA_VERSION_V2);
}

#[test]
fn test_migrate_emits_event_and_transforms_on_read() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    env.as_contract(&contract_id, || {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let v1 = WrapRecordV1 {
            timestamp: 2000,
            data_hash: BytesN::from_array(&env, &[71u8; 32]),
            archetype: symbol_short!("defi"),
            period,
        };
        env.storage().persistent().set(&wrap_key, &v1);
    });

    client.migrate(&SCHEMA_VERSION, &SCHEMA_VERSION_V2);
    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.period, period);
    assert_eq!(wrap.image_uri, String::from_str(&env, ""));
}

// ─── Opt-out privacy tests ────────────────────────────────────────────────────

#[test]
fn test_opt_out_hides_wraps_opt_in_reveals() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[60u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[80u8; 32]);
    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);

    assert!(client.get_wrap(&user, &period).is_some());
    assert!(!client.is_opted_out(&user));

    client.opt_out(&user);
    assert!(client.is_opted_out(&user));
    assert!(client.get_wrap(&user, &period).is_none());
    assert!(client.get_latest_wrap(&user).is_none());
    assert_eq!(client.balance_of(&user), 1);

    client.opt_in(&user);
    assert!(!client.is_opted_out(&user));
    assert!(client.get_wrap(&user, &period).is_some());
}

#[test]
fn test_opt_out_verify_data_still_works() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[61u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let data_json = Bytes::from_slice(&env, b"{\"score\":42}");
    let data_hash_raw = env.crypto().sha256(&data_json);
    let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());
    let archetype = symbol_short!("arch");
    let period = 2025u64;
    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash);
    client.mint_wrap(&user, &period, &archetype, &data_hash, &sig);

    client.opt_out(&user);
    assert!(client.verify_data(&user, &period, &data_json));
}

#[test]
fn test_admin_can_revoke_opted_out_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[62u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[81u8; 32]);
    let sig = sign_payload(&env, &signing_key, &contract_id, &user, period, &archetype, &hash);
    client.mint_wrap(&user, &period, &archetype, &hash, &sig);

    client.opt_out(&user);
    client.revoke_wrap(&user, &period);
    assert_eq!(client.balance_of(&user), 0);
}
