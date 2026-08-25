#![cfg(test)]

extern crate std;

use super::*;
use crate::test_utils::{sign_payload, sign_payload_versioned};
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{
        budget::ContractCostType,
        {Address as _, Events},
    },
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, TryIntoVal,
};
use std::vec::Vec;

const STRESS_USER_COUNT: usize = 128;

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
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &dummy_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &dummy_hash, &1u32, &signature);

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

    let period = 202401u64;
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

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);

    let events = crate::test_utils::decode_events(&env);
    let (topics, data) = events.last().expect("no events found");

    let _event_topic: Symbol = topics[0].try_into_val(&env).unwrap();
    let _event_user: Address = topics[1].try_into_val(&env).unwrap();
    let _event_period: u64 = topics[2].try_into_val(&env).unwrap();
    let (event_topic_data, event_user_data, _, event_archetype): (Symbol, Address, u64, Symbol) =
        data.try_into_val(&env).unwrap();
    assert_eq!(event_topic_data, Symbol::new(&env, "Mint"));
    assert_eq!(event_user_data, user);
    assert_eq!(event_archetype, archetype);
}

#[test]
fn test_revoke_emits_event_multi_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[14u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype_a = symbol_short!("gold");
    let archetype_b = symbol_short!("silvr");
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    let period_a = 202401u64;
    let period_b = 202402u64;

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period_a,
        &archetype_a,
        &hash,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period_b,
        &archetype_b,
        &hash,
    );

    client.mint_wrap(&user_a, &period_a, &archetype_a, &hash, &1u32, &sig_a);
    client.mint_wrap(&user_b, &period_b, &archetype_b, &hash, &1u32, &sig_b);
    let reason = BytesN::from_array(&env, &[0u8; 32]);
    client.revoke_wrap(&user_a, &period_a, &reason);

    // Each top-level invocation resets the event buffer in SDK 27, so capture
    // the event right after each revoke.
    let assert_revoke =
        |_client: &StellarWrapContractClient, env: &Env, user: &Address, period: u64| {
            let events = crate::test_utils::decode_events(env);
            let revoke_events: Vec<_> = events
                .iter()
                .filter(|(topics, _)| {
                    let sym: Symbol = topics[0].try_into_val(env).unwrap();
                    sym == symbol_short!("revoke")
                })
                .collect();
            assert_eq!(revoke_events.len(), 1);
            let (topics, data) = &revoke_events[0];
            let event_user: Address = topics[1].try_into_val(env).unwrap();
            let event_period: u64 = topics[2].try_into_val(env).unwrap();
            let event_reason: BytesN<32> = data.clone().try_into_val(env).unwrap();
            assert_eq!(event_user, *user);
            assert_eq!(event_period, period);
            assert_eq!(event_reason, reason);
        };

    assert_revoke(&client, &env, &user_a, period_a);
    client.revoke_wrap(&user_b, &period_b, &reason);
    assert_revoke(&client, &env, &user_b, period_b);
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
    let hash = BytesN::from_array(&env, &[0u8; 32]);

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202401,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202401, &archetype, &hash, &1u32, &sig1);

    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202402,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &202402, &archetype, &hash, &1u32, &sig2);

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
fn test_initialize_sets_storage_schema_version() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    assert_eq!(client.storage_schema_version(), 0);
    client.initialize(&admin, &pubkey);
    assert_eq!(client.storage_schema_version(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #52)")]
fn test_initialize_rejects_zero_admin_pubkey() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // An all-zero Ed25519 public key has no known private key; accepting it
    // would silently break every future mint, so initialization must reject it.
    let zero_pubkey = BytesN::from_array(&env, &[0u8; 32]);
    client.initialize(&admin, &zero_pubkey);
}

#[test]
fn test_initialize_after_rejected_zero_pubkey_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // The rejected zero-key attempt must not leave the contract half-initialized.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.initialize(&admin, &BytesN::from_array(&env, &[0u8; 32]));
    }));
    assert!(result.is_err(), "zero admin pubkey must be rejected");
    assert!(!client.health().initialized);

    // A subsequent valid initialization still succeeds.
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    client.initialize(&admin, &admin_pubkey);
    let health = client.health();
    assert!(health.initialized);
    assert!(health.has_admin);
    assert!(health.has_signing_key);
}

#[test]
fn test_health_reflects_initialization_state() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Before initialization: nothing configured.
    let health = client.health();
    assert!(!health.initialized);
    assert!(!health.has_admin);
    assert!(!health.has_signing_key);

    // Initialize the contract.
    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    // After initialization: everything configured.
    let health = client.health();
    assert!(health.initialized);
    assert!(health.has_admin);
    assert!(health.has_signing_key);
}

#[test]
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
    let period = 202401u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);
    let balance_before = client.balance_of(&user);
    let result = client.try_mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    assert!(result.is_err(), "duplicate period mint must fail");
    assert_eq!(client.balance_of(&user), balance_before);
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
fn test_update_admin_by_current_admin_succeeds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_admin",
            args: (&new_admin,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

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
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

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
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

    let tampered_data = Bytes::from_slice(&env, b"{\"score\":999}");
    assert!(!client.verify_data(&user, &period, &tampered_data));
}

#[test]
fn test_verify_data_corrupted_payload() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[6u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let original_data = Bytes::from_slice(&env, b"{\"valid\":true}");
    let data_hash_raw = env.crypto().sha256(&original_data);
    let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

    let corrupted_data = Bytes::from_slice(&env, b"\x00\xFF\xFE\xFDcorrupt\x01\x02");
    assert!(!client.verify_data(&user, &period, &corrupted_data));
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
    assert!(!client.verify_data(&user, &202401, &data));
}

#[test]
fn test_mint_wrap_rejects_period_tampered_signature() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[21u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let period_a = 202401u64;
    let period_b = 202402u64;

    // Sign the canonical payload for period A only.
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_a,
        &archetype,
        &data_hash,
    );

    // Submitting that signature with a different period must be rejected.
    let balance_before = client.balance_of(&user);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_wrap(
            &user,
            &period_b,
            &archetype,
            &data_hash,
            &CURRENT_PAYLOAD_VERSION,
            &signature,
        );
    }));
    assert!(
        result.is_err(),
        "signature for period A must not verify when period B is submitted"
    );
    assert_maps_to_invalid_signature(&result);

    // No wrap or wrap-count may be written for either period.
    assert!(client.get_wrap(&user, &period_a).is_none());
    assert!(client.get_wrap(&user, &period_b).is_none());
    assert_eq!(client.balance_of(&user), balance_before);
}

/// Asserts that a caught mint failure surfaced the contract's
/// `InvalidSignature` error (`Error(Contract, #5)`) rather than a raw host
/// `Crypto`/`InvalidInput` error.
fn assert_maps_to_invalid_signature(
    result: &std::result::Result<(), std::boxed::Box<dyn std::any::Any + Send>>,
) {
    let err = result
        .as_ref()
        .expect_err("the mint call must have failed")
        .downcast_ref::<std::string::String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        err.contains("Error(Contract, #5)"),
        "failed signature check must surface InvalidSignature (#5), got: {err}"
    );
}

#[test]
fn test_mint_wrap_rejects_signature_from_wrong_key() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[22u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let period = 202401u64;

    // Sign with a key other than the configured admin pubkey.
    let wrong_key = SigningKey::from_bytes(&[77u8; 32]);
    let signature = sign_payload(
        &env,
        &wrong_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &data_hash,
    );

    let balance_before = client.balance_of(&user);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.mint_wrap(
            &user,
            &period,
            &archetype,
            &data_hash,
            &CURRENT_PAYLOAD_VERSION,
            &signature,
        );
    }));
    assert_maps_to_invalid_signature(&result);

    // Nothing may be written by the failed mint.
    assert!(client.get_wrap(&user, &period).is_none());
    assert_eq!(client.balance_of(&user), balance_before);
}

#[test]
fn test_mint_wrap_rejects_zero_hash_submission_without_incrementing_balance() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[23u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let period = 202401u64;
    let signed_hash = BytesN::from_array(&env, &[42u8; 32]);
    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &signed_hash,
    );
    let balance_before = client.balance_of(&user);

    let result = client.try_mint_wrap(&user, &period, &archetype, &zero_hash, &1u32, &signature);

    assert!(result.is_err(), "zero hash mint must fail");
    assert_eq!(client.balance_of(&user), balance_before);
}

#[test]
fn test_get_wrap_existing_user_nonexistent_period() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[15u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    // Mint a wrap for the user at a specific period.
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let period = 202401u64;

    let signature = sign_payload_versioned(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
        CURRENT_PAYLOAD_VERSION,
    );
    client.mint_wrap(
        &user,
        &period,
        &archetype,
        &hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );

    // Verify the wrap exists for the minted period.
    assert!(client.get_wrap(&user, &period).is_some());

    // get_wrap should return None for a different, non-existent period.
    let nonexistent_period = 202402u64;
    assert!(client.get_wrap(&user, &nonexistent_period).is_none());
}

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
        202402,
        &archetype,
        &hash1,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202404,
        &archetype,
        &hash2,
    );
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202403,
        &archetype,
        &hash3,
    );

    client.mint_wrap(&user, &202402, &archetype, &hash1, &1u32, &sig1);
    client.mint_wrap(&user, &202404, &archetype, &hash2, &1u32, &sig2);
    client.mint_wrap(&user, &202403, &archetype, &hash3, &1u32, &sig3);

    let latest = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest.period, 202404);
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
    let period = 202501u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    let latest = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest.period, 202501);
    assert_eq!(latest.data_hash, hash);
}

#[test]
fn test_valid_period_boundaries() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let lower_hash = BytesN::from_array(&env, &[60u8; 32]);
    let upper_hash = BytesN::from_array(&env, &[61u8; 32]);

    let lower_sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202401,
        &archetype,
        &lower_hash,
    );
    let upper_sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        210012,
        &archetype,
        &upper_hash,
    );

    client.mint_wrap(&user, &202401, &archetype, &lower_hash, &1u32, &lower_sig);
    client.mint_wrap(&user, &210012, &archetype, &upper_hash, &1u32, &upper_sig);

    assert!(client.get_wrap(&user, &202401).is_some());
    assert!(client.get_wrap(&user, &210012).is_some());
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_invalid_period_zero_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[10u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[70u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 0u64;
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_invalid_period_one_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[11u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[71u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 1u64;
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_invalid_period_max_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[12u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[72u8; 32]);
    let archetype = symbol_short!("arch");
    let period = u64::MAX;
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);
}

#[test]
fn test_stress_mint_100_plus_unique_users() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[13u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let period = 202601u64;
    let mut users = Vec::with_capacity(STRESS_USER_COUNT);
    let mut cpu_samples = [0u64; STRESS_USER_COUNT];
    let mut mem_samples = [0u64; STRESS_USER_COUNT];

    for i in 0..STRESS_USER_COUNT {
        env.budget().reset_default();

        let user = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[i as u8; 32]);
        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &hash,
        );

        client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);

        cpu_samples[i] = env.budget().cpu_instruction_cost();
        mem_samples[i] = env.budget().memory_bytes_cost();
        users.push(user);
    }

    assert!(cpu_samples[0] > 0);
    assert!(mem_samples[0] > 0);
    assert!(cpu_samples.iter().all(|sample| *sample > 0));
    assert!(mem_samples.iter().all(|sample| *sample > 0));
    assert!(cpu_samples
        .iter()
        .skip(1)
        .any(|sample| *sample != cpu_samples[0]));
    assert!(mem_samples
        .iter()
        .skip(1)
        .any(|sample| *sample != mem_samples[0]));

    env.budget().reset_unlimited();

    for (i, user) in users.iter().enumerate() {
        let expected_hash = BytesN::from_array(&env, &[i as u8; 32]);
        let wrap = client.get_wrap(user, &period).unwrap();

        assert_eq!(wrap.period, period);
        assert_eq!(wrap.data_hash, expected_hash);
        assert_eq!(client.balance_of(user), 1);
        assert_eq!(client.get_latest_wrap(user).unwrap().period, period);
    }
}

#[test]
fn test_non_monotonic_period_mints_across_users() {
    // Two independent users mint periods in mixed, out-of-order sequences.
    // Each user's latest period and balance must be tracked independently,
    // and must reflect the highest period that user minted regardless of the
    // order in which the mints happened (or how they interleave with the
    // other user's mints).
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[14u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");

    // Helper to sign and mint in one step.
    let mint = |user: &Address, period: u64, tag: u8| {
        let hash = BytesN::from_array(&env, &[tag; 32]);
        let sig = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            user,
            period,
            &archetype,
            &hash,
        );
        client.mint_wrap(user, &period, &archetype, &hash, &1u32, &sig);
    };

    // Interleaved, non-monotonic ordering across both users:
    //   user_a: 202406 (high first), then 202401 (lower), then 202404 (middle)
    //   user_b: 202402 (low first), then 202412 (highest), then 202403
    mint(&user_a, 202406, 1); // a's latest -> 202406
    mint(&user_b, 202402, 2); // b's latest -> 202402
    mint(&user_a, 202401, 3); // older than a's latest; latest stays 202406
    mint(&user_b, 202412, 4); // b's latest -> 202412
    mint(&user_a, 202404, 5); // between a's periods; latest stays 202406
    mint(&user_b, 202403, 6); // older than b's latest; latest stays 202412

    // Each user's latest period is independent and equals their own maximum.
    let latest_a = client.get_latest_wrap(&user_a).unwrap();
    let latest_b = client.get_latest_wrap(&user_b).unwrap();
    assert_eq!(latest_a.period, 202406);
    assert_eq!(latest_b.period, 202412);

    // The latest record also carries that user's own data, not the other's.
    assert_eq!(latest_a.data_hash, BytesN::from_array(&env, &[1u8; 32]));
    assert_eq!(latest_b.data_hash, BytesN::from_array(&env, &[4u8; 32]));

    // Balances (mint counts) are independent per user.
    assert_eq!(client.balance_of(&user_a), 3);
    assert_eq!(client.balance_of(&user_b), 3);

    // Every individual period a user minted is retrievable and isolated to
    // that user; the other user never has it.
    for period in [202406u64, 202401, 202404] {
        assert!(client.get_wrap(&user_a, &period).is_some());
        assert!(client.get_wrap(&user_b, &period).is_none());
    }
    for period in [202402u64, 202412, 202403] {
        assert!(client.get_wrap(&user_b, &period).is_some());
        assert!(client.get_wrap(&user_a, &period).is_none());
    }
}

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

    client.mint_wrap(&user, &202401, &archetype, &hash, &1u32, &sig);
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

#[test]
fn test_migrate_applies_once_per_version() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    assert_eq!(client.migration_version(), 0);

    client.migrate(&1);
    assert_eq!(client.migration_version(), 1);

    client.migrate(&2);
    assert_eq!(client.migration_version(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_migrate_rejects_replay() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.migrate(&1);
    client.migrate(&1);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_migrate_before_init_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    env.mock_all_auths();

    client.migrate(&1);
}

#[test]
fn test_get_mint_timestamp_exists() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[14u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &dummy_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &dummy_hash, &1u32, &signature);

    let wrap = client.get_wrap(&user, &period).unwrap();
    assert_eq!(
        client.get_mint_timestamp(&user, &period),
        Some(wrap.timestamp)
    );
}

#[test]
fn test_get_mint_timestamp_missing() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let period = 202401u64;

    assert_eq!(client.get_mint_timestamp(&user, &period), None);
}

// ============================================================================
// burn_wrap tests
// ============================================================================

#[test]
fn test_burn_wrap_removes_wrap_from_storage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[20u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;
    let signature = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    // Mint a wrap
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &signature);
    assert!(client.get_wrap(&user, &period).is_some());

    // Burn the wrap
    client.burn_wrap(&user, &period);

    // Verify wrap is gone
    assert!(client.get_wrap(&user, &period).is_none());
}

#[test]
fn test_burn_wrap_decrements_count() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[21u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period1 = 202401u64;
    let period2 = 202402u64;

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period1,
        &archetype,
        &hash,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period2,
        &archetype,
        &hash,
    );

    // Mint two wraps
    client.mint_wrap(&user, &period1, &archetype, &hash, &1u32, &sig1);
    client.mint_wrap(&user, &period2, &archetype, &hash, &1u32, &sig2);
    assert_eq!(client.balance_of(&user), 2);

    // Burn one wrap
    client.burn_wrap(&user, &period1);

    // Count should be decremented
    assert_eq!(client.balance_of(&user), 1);

    // Other wrap should still exist
    assert!(client.get_wrap(&user, &period2).is_some());
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_burn_wrap_requires_owner_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[22u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &hash,
    );

    // User A mints a wrap
    client.mint_wrap(&user_a, &period, &archetype, &hash, &1u32, &sig);

    // User B tries to burn User A's wrap — should fail
    client.burn_wrap(&user_b, &period);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_burn_wrap_fails_for_nonexistent_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let user = Address::generate(&env);
    // Try to burn a wrap that was never created
    client.burn_wrap(&user, &202401);
}

#[test]
fn test_burn_wrap_emits_burn_event() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[23u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    // Clear events from mint
    env.events().all();

    // Burn the wrap
    client.burn_wrap(&user, &period);

    // Check the burn event
    let events = crate::test_utils::decode_events(&env);
    let (topics, data) = events.last().expect("no events found");

    let event_topic: Symbol = topics[0].try_into_val(&env).unwrap();
    let event_user: Address = topics[1].try_into_val(&env).unwrap();
    let event_period: u64 = topics[2].try_into_val(&env).unwrap();
    let event_owner: Address = data.try_into_val(&env).unwrap();

    assert_eq!(event_topic, symbol_short!("burn"));
    assert_eq!(event_user, user);
    assert_eq!(event_period, period);
    assert_eq!(event_owner, user);
}

#[test]
fn test_burn_wrap_owner_cannot_access_after() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[24u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);
    let record_before = client.get_wrap(&user, &period).unwrap();
    assert_eq!(record_before.data_hash, hash);

    // Burn the wrap
    client.burn_wrap(&user, &period);

    // Try to access the wrap — should return None
    assert!(client.get_wrap(&user, &period).is_none());

    // Verify data verification fails
    let data = Bytes::from_slice(&env, b"test");
    assert!(!client.verify_data(&user, &period, &data));
}

#[test]
fn test_burn_wrap_only_deletes_target() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[25u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period_a = 202401u64;
    let period_b = 202402u64;

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_a,
        &archetype,
        &hash,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period_b,
        &archetype,
        &hash,
    );

    // Mint two wraps
    client.mint_wrap(&user, &period_a, &archetype, &hash, &1u32, &sig_a);
    client.mint_wrap(&user, &period_b, &archetype, &hash, &1u32, &sig_b);

    // Burn wrap A
    client.burn_wrap(&user, &period_a);

    // Wrap A should be gone
    assert!(client.get_wrap(&user, &period_a).is_none());

    // Wrap B should still exist
    assert!(client.get_wrap(&user, &period_b).is_some());
}

#[test]
fn test_burn_wrap_clears_latest_period() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[26u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period1 = 202401u64;
    let period2 = 202402u64;

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period1,
        &archetype,
        &hash,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period2,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user, &period1, &archetype, &hash, &1u32, &sig1);
    client.mint_wrap(&user, &period2, &archetype, &hash, &1u32, &sig2);

    // Latest wrap should be period2
    let latest_before = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest_before.period, period2);

    // Burn the latest wrap
    client.burn_wrap(&user, &period2);

    // Latest wrap should now be period1
    let latest_after = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest_after.period, period1);
}

#[test]
fn test_burn_wrap_multiple_users_independent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[27u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("arch");
    let period = 202401u64;

    let sig_a = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        period,
        &archetype,
        &hash,
    );
    let sig_b = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        period,
        &archetype,
        &hash,
    );

    // Both users mint for the same period
    client.mint_wrap(&user_a, &period, &archetype, &hash, &1u32, &sig_a);
    client.mint_wrap(&user_b, &period, &archetype, &hash, &1u32, &sig_b);

    // Burn user A's wrap
    client.burn_wrap(&user_a, &period);

    // User A's wrap should be gone
    assert!(client.get_wrap(&user_a, &period).is_none());

    // User B's wrap should still exist (independent)
    assert!(client.get_wrap(&user_b, &period).is_some());
}

// ============================================================================
// get_all_wraps_for_user tests
// ============================================================================

#[test]
fn test_get_all_wraps_for_user_returns_all_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[30u8; 32]);
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
        202401,
        &archetype,
        &hash1,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202402,
        &archetype,
        &hash2,
    );
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202403,
        &archetype,
        &hash3,
    );

    client.mint_wrap(&user, &202401, &archetype, &hash1, &1u32, &sig1);
    client.mint_wrap(&user, &202402, &archetype, &hash2, &1u32, &sig2);
    client.mint_wrap(&user, &202403, &archetype, &hash3, &1u32, &sig3);

    let all_wraps = client.get_all_wraps_for_user(&user);
    assert_eq!(all_wraps.len(), 3);

    let periods: Vec<u64> = all_wraps.iter().map(|w| w.period).collect();
    assert!(periods.contains(&202401));
    assert!(periods.contains(&202402));
    assert!(periods.contains(&202403));
}

#[test]
fn test_get_all_wraps_for_user_empty_for_no_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    client.initialize(&admin, &pubkey);

    let user = Address::generate(&env);
    let all_wraps = client.get_all_wraps_for_user(&user);
    assert_eq!(all_wraps.len(), 0);
}

#[test]
fn test_get_all_wraps_for_user_single_wrap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[31u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("solo");
    let period = 202401u64;

    let sig = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user,
        period,
        &archetype,
        &hash,
    );
    client.mint_wrap(&user, &period, &archetype, &hash, &1u32, &sig);

    let all_wraps = client.get_all_wraps_for_user(&user);
    assert_eq!(all_wraps.len(), 1);
    let wrap = all_wraps.get(0).unwrap();
    assert_eq!(wrap.period, period);
    assert_eq!(wrap.data_hash, hash);
    assert_eq!(wrap.archetype, archetype);
}

#[test]
fn test_get_all_wraps_for_user_independent_per_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[32u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[42u8; 32]);

    let sig_a1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        202401,
        &archetype,
        &hash,
    );
    let sig_a2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_a,
        202402,
        &archetype,
        &hash,
    );
    let sig_b1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user_b,
        202401,
        &archetype,
        &hash,
    );

    client.mint_wrap(&user_a, &202401, &archetype, &hash, &1u32, &sig_a1);
    client.mint_wrap(&user_a, &202402, &archetype, &hash, &1u32, &sig_a2);
    client.mint_wrap(&user_b, &202401, &archetype, &hash, &1u32, &sig_b1);

    let wraps_a = client.get_all_wraps_for_user(&user_a);
    let wraps_b = client.get_all_wraps_for_user(&user_b);

    assert_eq!(wraps_a.len(), 2);
    assert_eq!(wraps_b.len(), 1);
}

/// Comprehensive unit tests for verify_data function.
/// Tests the core requirement: verify_data must return true for correct data payloads
/// that match the hash stored during minting.
mod verify_data_unit_tests {
    use super::*;
    use std::vec;

    /// Helper function to set up a standard test environment
    /// Returns: (Env, contract_id, client, signing_key, admin, user)
    fn setup_env() -> (
        Env,
        Address,
        StellarWrapContractClient<'static>,
        SigningKey,
        Address,
        Address,
    ) {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarWrapContract);
        let client = StellarWrapContractClient::new(&env, &contract_id);

        let signing_key = SigningKey::from_bytes(&[15u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        client.initialize(&admin, &admin_pubkey);
        env.mock_all_auths();

        (env, contract_id, client, signing_key, admin, user)
    }

    /// PRIMARY TEST: Verifies that verify_data returns true when given a correct,
    /// well-formed data payload that matches the stored hash.
    /// This is the core requirement from issue #483.
    #[test]
    fn verify_data_succeeds_with_correct_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Build a correct payload matching JSON data
        let correct_payload = Bytes::from_slice(
            &env,
            b"{\"user_id\":123,\"status\":\"active\",\"level\":10}",
        );

        // Compute the hash that will be stored in the wrap record
        let data_hash_raw = env.crypto().sha256(&correct_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("gold");
        let period = 202401u64;

        // Sign and mint the wrap with this hash
        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Verify that passing the exact same correct payload returns true
        let result = client.verify_data(&user, &period, &correct_payload);
        assert!(
            result,
            "verify_data must return true for the correct payload that matches the stored hash"
        );
    }

    /// Verifies that verify_data handles complex JSON payloads correctly
    /// and maintains deterministic behavior with correct data.
    #[test]
    fn verify_data_succeeds_with_complex_json_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Build a complex, nested JSON payload
        let complex_payload = Bytes::from_slice(
            &env,
            b"{\"profile\":{\"name\":\"Alice\",\"age\":30},\"scores\":[100,95,87],\"metadata\":{\"created\":\"2024-01-15\",\"verified\":true}}"
        );

        let data_hash_raw = env.crypto().sha256(&complex_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("standard");
        let period = 202402u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Verify the exact same complex payload succeeds
        let result = client.verify_data(&user, &period, &complex_payload);
        assert!(
            result,
            "verify_data must correctly verify complex JSON payloads"
        );
    }

    /// Verifies that verify_data rejects a payload with modified content
    #[test]
    fn verify_data_fails_with_incorrect_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        let original_payload = Bytes::from_slice(&env, b"{\"score\":100,\"rank\":\"gold\"}");

        let data_hash_raw = env.crypto().sha256(&original_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("verify");
        let period = 202403u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Try to verify with a tampered/different payload
        let incorrect_payload = Bytes::from_slice(&env, b"{\"score\":999,\"rank\":\"platinum\"}");
        let result = client.verify_data(&user, &period, &incorrect_payload);

        assert!(
            !result,
            "verify_data must return false for incorrect/tampered payload"
        );
    }

    /// Verifies that verify_data handles empty/minimal payloads correctly
    #[test]
    fn verify_data_handles_minimal_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Create a minimal empty JSON object payload
        let minimal_payload = Bytes::from_slice(&env, b"{}");

        let data_hash_raw = env.crypto().sha256(&minimal_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("minimal");
        let period = 202404u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Verify that the minimal payload works correctly
        let result = client.verify_data(&user, &period, &minimal_payload);
        assert!(
            result,
            "verify_data must correctly handle minimal/empty payloads"
        );
    }

    /// Verifies that verify_data is deterministic —
    /// calling verify_data multiple times with the same correct payload
    /// always returns the same result
    #[test]
    fn verify_data_is_deterministic_with_correct_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        let payload = Bytes::from_slice(&env, b"{\"deterministic\":true,\"value\":42}");

        let data_hash_raw = env.crypto().sha256(&payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("determ");
        let period = 202405u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Call verify_data multiple times with the same payload
        let result1 = client.verify_data(&user, &period, &payload);
        let result2 = client.verify_data(&user, &period, &payload);
        let result3 = client.verify_data(&user, &period, &payload);

        assert!(result1, "First call must succeed");
        assert!(result2, "Second call must succeed");
        assert!(result3, "Third call must succeed");
        assert_eq!(result1, result2, "verify_data must be deterministic");
        assert_eq!(result2, result3, "verify_data must be deterministic");
    }

    /// Verifies that verify_data correctly distinguishes between
    /// different correct payloads for different periods
    #[test]
    fn verify_data_distinguishes_different_periods() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        let payload1 = Bytes::from_slice(&env, b"{\"period\":1,\"data\":\"first\"}");
        let payload2 = Bytes::from_slice(&env, b"{\"period\":2,\"data\":\"second\"}");

        let hash1_raw = env.crypto().sha256(&payload1);
        let hash1 = BytesN::from_array(&env, &hash1_raw.to_array());

        let hash2_raw = env.crypto().sha256(&payload2);
        let hash2 = BytesN::from_array(&env, &hash2_raw.to_array());

        let archetype = symbol_short!("multi");
        let period1 = 202406u64;
        let period2 = 202407u64;

        let sig1 = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period1,
            &archetype,
            &hash1,
        );
        let sig2 = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period2,
            &archetype,
            &hash2,
        );

        client.mint_wrap(&user, &period1, &archetype, &hash1, &1u32, &sig1);
        client.mint_wrap(&user, &period2, &archetype, &hash2, &1u32, &sig2);

        // Verify correct payload for each period
        assert!(
            client.verify_data(&user, &period1, &payload1),
            "Period 1 must verify with its correct payload"
        );
        assert!(
            client.verify_data(&user, &period2, &payload2),
            "Period 2 must verify with its correct payload"
        );

        // Cross-verify should fail (wrong payload for period)
        assert!(
            !client.verify_data(&user, &period1, &payload2),
            "Period 1 must reject payload from period 2"
        );
        assert!(
            !client.verify_data(&user, &period2, &payload1),
            "Period 2 must reject payload from period 1"
        );
    }

    /// Verifies that verify_data handles binary data (non-UTF8) correctly
    #[test]
    fn verify_data_succeeds_with_binary_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Create binary payload with non-UTF8 bytes
        let binary_payload = Bytes::from_slice(&env, b"\x00\x01\x02\xFF\xFE\xFD");

        let data_hash_raw = env.crypto().sha256(&binary_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("binary");
        let period = 202408u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Verify that the exact binary payload matches
        let result = client.verify_data(&user, &period, &binary_payload);
        assert!(result, "verify_data must correctly verify binary payloads");
    }

    /// Verifies that verify_data rejects binary data with even a single bit difference
    #[test]
    fn verify_data_fails_with_single_byte_difference() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        let original_binary = Bytes::from_slice(&env, b"\x00\x01\x02\x03\x04");

        let data_hash_raw = env.crypto().sha256(&original_binary);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("bitdiff");
        let period = 202409u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Try with one byte changed (0x02 -> 0x03 at index 2)
        let tampered_binary = Bytes::from_slice(&env, b"\x00\x01\x03\x03\x04");
        let result = client.verify_data(&user, &period, &tampered_binary);

        assert!(
            !result,
            "verify_data must reject payload with even a single byte changed"
        );
    }

    /// Verifies that verify_data handles very large payloads correctly
    #[test]
    fn verify_data_succeeds_with_large_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Create a large payload (1KB of repeated data)
        let mut large_bytes = vec![0u8; 1024];
        for (i, byte) in large_bytes.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }
        let large_payload = Bytes::from_slice(&env, &large_bytes);

        let data_hash_raw = env.crypto().sha256(&large_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("large");
        let period = 202410u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Verify the large payload matches
        let result = client.verify_data(&user, &period, &large_payload);
        assert!(result, "verify_data must correctly verify large payloads");
    }

    /// Covers issue #256: large off-chain JSON payloads should not surprise
    /// budget usage or hash verification behavior.
    #[test]
    fn verify_data_succeeds_with_representative_large_payload() {
        let (env, contract_id, client, signing_key, _admin, user) = setup_env();

        // Build a representative off-chain JSON report (~200KB) made of many
        // repeated records, mimicking a real analytics batch payload.
        let mut large_bytes: Vec<u8> = Vec::new();
        for i in 0..4000u64 {
            let line = std::format!(
                "{{\"record\":{},\"user\":\"u-{}\",\"status\":\"processed\",\"score\":99.5}}\n",
                i,
                i
            );
            large_bytes.extend_from_slice(line.as_bytes());
        }
        assert!(
            large_bytes.len() > 100_000,
            "payload must be a representative large size, got {}",
            large_bytes.len()
        );
        let large_payload = Bytes::from_slice(&env, &large_bytes);

        let data_hash_raw = env.crypto().sha256(&large_payload);
        let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

        let archetype = symbol_short!("large");
        let period = 202501u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
        );
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &signature);

        // Reset the cost trackers so the budget below reflects only `verify_data`.
        env.budget().reset_tracker();
        let cpu_before = env.budget().cpu_instruction_cost();

        let result = client.verify_data(&user, &period, &large_payload);

        let cpu_after = env.budget().cpu_instruction_cost();
        let sha_tracker = env.budget().tracker(ContractCostType::ComputeSha256Hash);

        assert!(
            result,
            "verify_data must accept the exact large payload bytes"
        );
        assert!(
            cpu_after > cpu_before,
            "verify_data must consume cpu budget for a large payload ({} -> {})",
            cpu_before,
            cpu_after
        );
        assert!(
            sha_tracker
                .inputs
                .is_some_and(|n| n >= large_bytes.len() as u64),
            "sha256 must hash the full payload (inputs = {:?})",
            sha_tracker.inputs
        );
    }
}

/// `balance_of` returns zero for any user before `initialize` is called.
///
/// This is intentional and safe: `balance_of` is a read-only query that
/// reads the `WrapCount` persistent storage key for the user, returning
/// `unwrap_or(0)` when the key is absent. Because the function neither
/// mutates state nor requires authorization, there is no security risk in
/// allowing calls before the contract is initialized. The behavior mirrors
/// standard token semantics where a new address has zero balance.
#[test]
fn test_balance_of_before_initialize() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);

    // Contract is NOT initialized — no admin, no signing key.
    // balance_of must still return 0 without panicking.
    assert_eq!(client.balance_of(&user), 0);
}

#[test]
fn test_get_latest_wrap_multiple_wraps() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[99u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let archetype = symbol_short!("arch");
    let hash1 = BytesN::from_array(&env, &[10u8; 32]);
    let hash2 = BytesN::from_array(&env, &[20u8; 32]);
    let hash3 = BytesN::from_array(&env, &[30u8; 32]);

    let sig1 = sign_payload_versioned(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202401,
        &archetype,
        &hash1,
        CURRENT_PAYLOAD_VERSION,
    );
    let sig2 = sign_payload_versioned(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202402,
        &archetype,
        &hash2,
        CURRENT_PAYLOAD_VERSION,
    );
    let sig3 = sign_payload_versioned(
        &env,
        &signing_key,
        &contract_id,
        &user,
        202403,
        &archetype,
        &hash3,
        CURRENT_PAYLOAD_VERSION,
    );

    client.mint_wrap(
        &user,
        &202401,
        &archetype,
        &hash1,
        &CURRENT_PAYLOAD_VERSION,
        &sig1,
    );
    let latest1 = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest1.period, 202401);
    assert_eq!(latest1.data_hash, hash1);

    client.mint_wrap(
        &user,
        &202402,
        &archetype,
        &hash2,
        &CURRENT_PAYLOAD_VERSION,
        &sig2,
    );
    let latest2 = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest2.period, 202402);
    assert_eq!(latest2.data_hash, hash2);

    client.mint_wrap(
        &user,
        &202403,
        &archetype,
        &hash3,
        &CURRENT_PAYLOAD_VERSION,
        &sig3,
    );
    let latest3 = client.get_latest_wrap(&user).unwrap();
    assert_eq!(latest3.period, 202403);
    assert_eq!(latest3.data_hash, hash3);

    assert_eq!(client.balance_of(&user), 3);
}
