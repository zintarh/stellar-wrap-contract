#![cfg(test)]
extern crate std;

use super::*;
use crate::constants::{TOKEN_NAME, TOKEN_SYMBOL, TOKEN_DECIMALS, CONTRACT_DESCRIPTION, VERSION};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, TryIntoVal,
};
use std::{
    format,
    panic::{catch_unwind, AssertUnwindSafe},
};


fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    expiry_ledger: u32,
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

fn register_test_archetypes(client: &StellarWrapContractClient) {
    client.add_archetype(&symbol_short!("arch"));
    client.add_archetype(&symbol_short!("builder"));
    client.add_archetype(&symbol_short!("defi"));
    client.add_archetype(&symbol_short!("soroban"));
    client.add_archetype(&symbol_short!("architect"));
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );


    let events = env.events().all();
    let last_event = events.last().expect("No events found");
    let (_, topics, data) = last_event;

    assert_eq!(event_topic, symbol_short!("mint"));
    assert_eq!(event_user, user);
    assert_eq!(event_period, period);
    assert_eq!(event_archetype, archetype);
}

#[test]
fn test_streak_after_first_mint_is_one() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[13u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let period = 202402u64;
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
        u32::MAX,
    );

    assert_eq!(client.get_streak(&user), 1);
}

#[test]
fn test_streak_increments_for_consecutive_months_and_year_boundary() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[14u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[2u8; 32]);

    let signature_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202412u64,
        &archetype,
        &hash,
        u32::MAX,
    );
    assert_eq!(client.get_streak(&user), 1);

    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202501u64,
        &archetype,
        &hash,
        u32::MAX,
    );
    assert_eq!(client.get_streak(&user), 2);
}

#[test]
fn test_streak_resets_after_gap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[15u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[3u8; 32]);

    let signature_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202501u64,
        &archetype,
        &hash,
        u32::MAX,
    );
    assert_eq!(client.get_streak(&user), 1);

    let signature_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202503u64,
        &archetype,
        &hash,
        u32::MAX,
    );
    assert_eq!(client.get_streak(&user), 1);
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2022,
        &archetype,
        &hash,
        u32::MAX,
    );

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

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

    assert_eq!(client.decimals(), TOKEN_DECIMALS);
    assert_eq!(client.name(), String::from_str(&env, TOKEN_NAME));
    assert_eq!(client.symbol(), String::from_str(&env, TOKEN_SYMBOL));
}

/// Regression test: Verify `name()`, `symbol()`, and `decimals()` return stable
/// values across contract upgrades. These functions are pure view functions that
/// must not change after a WASM upgrade to maintain indexer compatibility.
///
/// NOTE: These functions return constant values hardcoded in the contract.
/// Changing TOKEN_NAME, TOKEN_SYMBOL, or TOKEN_DECIMALS is a BREAKING CHANGE
/// for indexers and downstream consumers that cache these values.
#[test]
fn test_token_metadata_preserved_after_upgrade() {
    let env = Env::default();

    let contract_v1_id = env.register_contract(None, StellarWrapContract);
    let client_v1 = StellarWrapContractClient::new(&env, &contract_v1_id);

    assert_eq!(client_v1.decimals(), TOKEN_DECIMALS, "V1 decimals");
    assert_eq!(client_v1.name(), String::from_str(&env, TOKEN_NAME), "V1 name");
    assert_eq!(client_v1.symbol(), String::from_str(&env, TOKEN_SYMBOL), "V1 symbol");

    let contract_v2_id = env.register_contract(None, StellarWrapContract);
    let client_v2 = StellarWrapContractClient::new(&env, &contract_v2_id);

    assert_eq!(client_v2.decimals(), TOKEN_DECIMALS, "V2 decimals must match V1");
    assert_eq!(client_v2.name(), String::from_str(&env, TOKEN_NAME), "V2 name must match V1");
    assert_eq!(client_v2.symbol(), String::from_str(&env, TOKEN_SYMBOL), "V2 symbol must match V1");

    assert_eq!(client_v1.decimals(), client_v2.decimals(), "decimals stable across versions");
    assert_eq!(client_v1.name(), client_v2.name(), "name stable across versions");
    assert_eq!(client_v1.symbol(), client_v2.symbol(), "symbol stable across versions");
}

// ─── Issue #48: version tests ───────────────────────────────────────────────

#[test]
fn test_version_returns_expected_value() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    assert_eq!(client.version(), VERSION);
}

// ─── Issue #56: contract_info tests ─────────────────────────────────────────

#[test]
fn test_contract_info_returns_correct_fields() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let info = client.contract_info();
    assert_eq!(info.name, String::from_str(&env, TOKEN_NAME));
    assert_eq!(info.version, String::from_str(&env, "0.1.0"));
    assert_eq!(
        info.repo,
        String::from_str(&env, "https://github.com/zintarh/stellar-wrap-contract")
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

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

    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period,
        &archetype,
        &hash_b,
        u32::MAX,
    );

    // Both mints for the same period should succeed

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

#[test]
fn test_same_user_mint_different_periods_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[11u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let hash_a = BytesN::from_array(&env, &[11u8; 32]);
    let hash_b = BytesN::from_array(&env, &[22u8; 32]);
    let period_a = 202512u64;
    let period_b = 202601u64;

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_a,
        &archetype,
        &hash_a,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_b,
        &archetype,
        &hash_b,
    );

    client.mint_wrap(&user, &period_a, &archetype, &hash_a, &sig_a);
    client.mint_wrap(&user, &period_b, &archetype, &hash_b, &sig_b);

    assert!(client.get_wrap(&user, &period_a).is_some());
    assert!(client.get_wrap(&user, &period_b).is_some());
    assert_eq!(client.balance_of(&user), 2);
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

    let events = env.events().all();
    let last_event = events.last().expect("Expected at least one event");
    let (event_contract, topics, data) = last_event;

    // Verify event is emitted by the correct contract
    assert_eq!(event_contract, contract_id);

    let topic_1: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let topic_2: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let topic_3: u64 = topics.get(3).unwrap().try_into_val(&env).unwrap();

    assert_eq!(topic_0, contract_id, "Topic 0 must be the contract Address");
    assert_eq!(
    assert_eq!(topic_2, user, "Topic 2 must be the user Address");
    assert_eq!(topic_3, period, "Topic 3 must be the period u64");

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period_b,
        &archetype_b,
        &hash_b,
        u32::MAX,
    );


    let events = env.events().all();

    // Collect mint events emitted by our contract
    let mut mint_events = soroban_sdk::vec![&env];
    for event in events.iter() {
        let (addr, topics, _data) = &event;
        if *addr == contract_id && topics.len() == 4 {
            let t: Result<Symbol, _> = topics.get(1).unwrap().try_into_val(&env);
            if t.map_or(false, |s| s == symbol_short!("mint")) {
                mint_events.push_back(event.clone());
            }
        }
    }

    assert_eq!(mint_events.len(), 2, "Expected exactly 2 mint events");

    // Verify first mint event (user_a)
    let (_, topics_a, data_a) = mint_events.get(0).unwrap();
    let ev_user_a: Address = topics_a.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_period_a: u64 = topics_a.get(3).unwrap().try_into_val(&env).unwrap();
    let ev_arch_a: Symbol = data_a.try_into_val(&env).unwrap();
    assert_eq!(ev_version, symbol_short!("v1"));
    assert_eq!(ev_user_a, user_a);
    assert_eq!(ev_period_a, period_a);
    assert_eq!(ev_arch_a, archetype_a);

    // Verify second mint event (user_b)
    let (_, topics_b, data_b) = mint_events.get(1).unwrap();
    let ev_user_b: Address = topics_b.get(2).unwrap().try_into_val(&env).unwrap();
    let ev_period_b: u64 = topics_b.get(3).unwrap().try_into_val(&env).unwrap();
    let ev_arch_b: Symbol = data_b.try_into_val(&env).unwrap();
    assert_eq!(ev_version, symbol_short!("v1"));
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

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

// ─── Issue #93: compute_data_hash tests ─────────────────────────────────────

#[test]
fn test_compute_data_hash_matches_mint_wrap_stored_hash() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let data_json = Bytes::from_slice(&env, b"{\"score\":100,\"level\":\"gold\"}");
    let computed = client.compute_data_hash(&data_json);
    let archetype = symbol_short!("arch");
    let period = 2024u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &computed,
    );
    client.mint_wrap(&user, &period, &archetype, &computed, &0u64, &signature);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(wrap.data_hash, computed);
}

#[test]
fn test_compute_data_hash_matches_off_chain_sha256() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let data_json = Bytes::from_slice(&env, b"{\"persona\":\"builder\",\"tx_count\":42}");
    let on_chain = client.compute_data_hash(&data_json);
    let off_chain_raw = env.crypto().sha256(&data_json);
    let off_chain = BytesN::from_array(&env, &off_chain_raw.to_array());

    assert_eq!(on_chain, off_chain);
    assert!(client.verify_data(&Address::generate(&env), &9999, &data_json) == false);
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2024,
        &archetype,
        &hash2,
        u32::MAX,
    );
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        2023,
        &archetype,
        &hash3,
        u32::MAX,
    );


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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

    let latest = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest.period, 2025);
    assert_eq!(latest.data_hash, hash);
}

// ─── Issue #138: compare_wraps tests ────────────────────────────────────────

#[test]
fn test_compare_wraps_both_users_have_wraps_same_archetype() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[40u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype = symbol_short!("arch");
    let hash_a = BytesN::from_array(&env, &[40u8; 32]);
    let hash_b = BytesN::from_array(&env, &[41u8; 32]);

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

    client.mint_wrap(&user_a, &period, &archetype, &hash_a, &sig_a);
    client.mint_wrap(&user_b, &period, &archetype, &hash_b, &sig_b);

    let comparison = client.compare_wraps(&user_a, &user_b, &period);
    assert_eq!(comparison.period, period);
    assert!(comparison.both_have_wrap);
    assert!(comparison.same_archetype);
    assert_eq!(
        comparison.user_a_wrap,
        WrapRecordOption::Some(client.get_wrap(&user_a, &period).unwrap())
    );
    assert_eq!(
        comparison.user_b_wrap,
        WrapRecordOption::Some(client.get_wrap(&user_b, &period).unwrap())
    );
}

#[test]
fn test_compare_wraps_both_users_have_wraps_different_archetypes() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[41u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype_a = symbol_short!("arch");
    let archetype_b = symbol_short!("defi");
    let hash_a = BytesN::from_array(&env, &[42u8; 32]);
    let hash_b = BytesN::from_array(&env, &[43u8; 32]);

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype_a,
        &hash_a,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period,
        &archetype_b,
        &hash_b,
    );

    client.mint_wrap(&user_a, &period, &archetype_a, &hash_a, &sig_a);
    client.mint_wrap(&user_b, &period, &archetype_b, &hash_b, &sig_b);

    let comparison = client.compare_wraps(&user_a, &user_b, &period);
    assert!(comparison.both_have_wrap);
    assert!(!comparison.same_archetype);
    assert_eq!(
        comparison.user_a_wrap,
        WrapRecordOption::Some(client.get_wrap(&user_a, &period).unwrap())
    );
    assert_eq!(
        comparison.user_b_wrap,
        WrapRecordOption::Some(client.get_wrap(&user_b, &period).unwrap())
    );
}

#[test]
fn test_compare_wraps_one_user_has_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[42u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let period = 2026u64;
    let archetype = symbol_short!("arch");
    let hash_a = BytesN::from_array(&env, &[44u8; 32]);
    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &hash_a,
    );

    client.mint_wrap(&user_a, &period, &archetype, &hash_a, &sig_a);

    let comparison = client.compare_wraps(&user_a, &user_b, &period);
    assert!(!comparison.both_have_wrap);
    assert!(!comparison.same_archetype);
    assert!(comparison.user_a_wrap.is_some());
    assert!(comparison.user_b_wrap.is_none());
}

#[test]
fn test_compare_wraps_neither_user_has_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[45u8; 32]);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &pubkey);

    let comparison = client.compare_wraps(&user_a, &user_b, &2026u64);
    assert!(!comparison.both_have_wrap);
    assert!(!comparison.same_archetype);
    assert!(comparison.user_a_wrap.is_none());
    assert!(comparison.user_b_wrap.is_none());
}

#[test]
fn test_compare_total_wraps_returns_lifetime_counts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[43u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let hash_a_1 = BytesN::from_array(&env, &[46u8; 32]);
    let hash_a_2 = BytesN::from_array(&env, &[47u8; 32]);
    let hash_b = BytesN::from_array(&env, &[48u8; 32]);

    let sig_a_1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        2024,
        &archetype,
        &hash_a_1,
    );
    let sig_a_2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        2025,
        &archetype,
        &hash_a_2,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        2026,
        &archetype,
        &hash_b,
    );

    client.mint_wrap(&user_a, &2024, &archetype, &hash_a_1, &sig_a_1);
    client.mint_wrap(&user_a, &2025, &archetype, &hash_a_2, &sig_a_2);
    client.mint_wrap(&user_b, &2026, &archetype, &hash_b, &sig_b);

    let totals = client.compare_total_wraps(&user_a, &user_b);
    assert_eq!(totals, (2, 1));
}

// ─── Issue #85: negative tests before initialize ────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_mint_wrap_before_init_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let archetype = symbol_short!("arch");
    let sig = BytesN::from_array(&env, &[0u8; 64]);

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );
    assert_eq!(client.balance_of(&user), 1);

    client.revoke_wrap(&user, &period);

    assert!(client.get_wrap(&user, &period).is_none());
    assert_eq!(client.balance_of(&user), 0);

    let events = env.events().all();
    let last_event = events.last().expect("Expected revoke event");
    let (_, topics, data) = last_event;

    let event_topic: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let event_user: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let event_period: u64 = topics.get(3).unwrap().try_into_val(&env).unwrap();
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
        u32::MAX,
    );

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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

    // Successful mint — guard must be set then cleared.
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
    register_test_archetypes(&client);

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
        u32::MAX,
    );

    // First mint succeeds.
    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    // Second mint with same (user, period) fails due to duplicate guard.
    let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);
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

    let topic_0: Address = topics.get(0).unwrap().try_into_val(&env).unwrap();
    let topic_1: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let topic_2: Symbol = topics.get(2).unwrap().try_into_val(&env).unwrap();
    assert_eq!(topic_1, symbol_short!("admin"));
    assert_eq!(topic_2, symbol_short!("updated"));

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
    register_test_archetypes(&client);

    // A hash with only the last byte set — not all-zero, should succeed
    let mut hash_bytes = [0u8; 32];
    hash_bytes[31] = 1;
    let edge_hash = BytesN::from_array(&env, &hash_bytes);
    let archetype = symbol_short!("arch");
    let period = 2024u64;


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
    register_test_archetypes(&client);

    let max_hash = BytesN::from_array(&env, &[0xff; 32]);
    let archetype = symbol_short!("arch");
    let period = 2024u64;


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
    register_test_archetypes(&client);

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);


    let before = client.get_wrap(&user, &period).unwrap();

    let new_hash = BytesN::from_array(&env, &[99u8; 32]);
    let new_arch = symbol_short!("builder");

    let after = client.get_wrap(&user, &period).unwrap();
    assert_eq!(
        after.timestamp, before.timestamp,
        "Original timestamp must be preserved"
    );
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
    register_test_archetypes(&client);

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let (_, topics, data) = last_event;

    let topic_1: Symbol = topics.get(1).unwrap().try_into_val(&env).unwrap();
    let topic_2: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();
    let topic_3: u64 = topics.get(3).unwrap().try_into_val(&env).unwrap();
    let ev_arch: Symbol = data.try_into_val(&env).unwrap();

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
    register_test_archetypes(&client);

    let new_hash = BytesN::from_array(&env, &[99u8; 32]);
    let new_arch = symbol_short!("arch");
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
    register_test_archetypes(&client);

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);

    // Reset auth — no admin auth mocked
    let env2 = Env::default();
    let contract_id2 = env2.register_contract(None, StellarWrapContract);
    let client2 = StellarWrapContractClient::new(&env2, &contract_id2);
    client2.initialize(&admin, &admin_pubkey);

    let new_hash = BytesN::from_array(&env2, &[99u8; 32]);
    let new_arch = symbol_short!("arch");
    let sig2 = sign_update_payload(
        &env2,
        &signing_key,
        &contract_id2,
        &user,
        period,
        &new_arch,
        &new_hash,
    );
    // No auth mocked — must panic
    client2.update_wrap(&user, &period, &new_hash, &new_arch, &sig2, &None);
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
    register_test_archetypes(&client);

    let period = 2025u64;
    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[41u8; 32]);
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);


// ─── Issue #53: has_wrap() tests ────────────────────────────────────────────

/// has_wrap() returns false before any wrap is minted for a user+period pair.
#[test]
fn test_has_wrap_returns_false_before_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[60u8; 32]);
    client.initialize(&admin, &pubkey);

    let user = Address::generate(&env);
    let period = 202406u64;

    assert!(!client.has_wrap(&user, &period));
}

/// has_wrap() returns true after a successful mint for the same user+period.
#[test]
fn test_has_wrap_returns_true_after_mint() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[61u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let period = 202406u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[61u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
        u32::MAX,
    );

    // Before mint: should not exist.
    assert!(!client.has_wrap(&user, &period));

    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);

    // After mint: must exist.
    assert!(client.has_wrap(&user, &period));
}

/// has_wrap() is period-scoped: true for minted period, false for different period.
#[test]
fn test_has_wrap_is_scoped_to_period() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[62u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let period_a = 202406u64;
    let period_b = 202407u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[62u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_a,
        &archetype,
        &data_hash,
        u32::MAX,
    );

    client.mint_wrap(&user, &period_a, &archetype, &data_hash, &signature);

    assert!(client.has_wrap(&user, &period_a));
    assert!(!client.has_wrap(&user, &period_b));
}

/// has_wrap() returns false after the wrap is revoked.
#[test]
fn test_has_wrap_returns_false_after_revoke() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[63u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let period = 202406u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[63u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
        u32::MAX,
    );

    client.mint_wrap(&user, &period, &archetype, &data_hash, &signature);
    assert!(client.has_wrap(&user, &period));

    client.revoke_wrap(&user, &period);
    assert!(!client.has_wrap(&user, &period));
}

/// has_wrap() is user-scoped: true for user A, false for user B on same period.
#[test]
fn test_has_wrap_is_scoped_to_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[64u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();
    register_test_archetypes(&client);

    let period = 202406u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[64u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &data_hash,
        u32::MAX,
    );

    client.mint_wrap(&user_a, &period, &archetype, &data_hash, &signature);

    assert!(client.has_wrap(&user_a, &period));
    assert!(!client.has_wrap(&user_b, &period));
}

    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

}
