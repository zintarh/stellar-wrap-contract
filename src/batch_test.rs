#![cfg(test)]

extern crate std;

use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short, testutils::Address as _, vec, Address, BytesN, Env, Symbol, TryIntoVal,
};

use super::*;
use crate::{
    storage_types::BatchWrapItem,
    test_utils::{decode_events, sign_payload},
};

#[test]
fn test_mint_wrap_batch_happy_path() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let initial_total = client.total_wrap_count();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let payload_version = 1u32;

    let sig1 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user1,
        period,
        &archetype,
        &data_hash,
    );
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user2,
        period,
        &archetype,
        &data_hash,
    );
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user3,
        period,
        &archetype,
        &data_hash,
    );

    let item1 = BatchWrapItem {
        user: user1.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: sig1,
    };
    let item2 = BatchWrapItem {
        user: user2.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: sig2,
    };
    let item3 = BatchWrapItem {
        user: user3.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: sig3,
    };

    let items = vec![&env, item1, item2, item3];

    client.mint_wrap_batch(&items, &None);

    // Acceptance criteria 1: A batch of 3 items for 3 distinct users succeeds.
    // (If it didn't succeed, it would have panicked above).

    // Acceptance criteria 3: total_wrap_count() increases by exactly the batch size.
    assert_eq!(client.total_wrap_count(), initial_total + 3);

    let users = [&user1, &user2, &user3];
    for user in users.iter() {
        // Acceptance criteria 2: Each user's get_wrap, balance_of, and get_latest_wrap match a single-mint baseline.
        let wrap = client.get_wrap(user, &period).expect("wrap missing");
        assert_eq!(wrap.data_hash, data_hash);
        assert_eq!(client.balance_of(user), 1);
        let latest = client.get_latest_wrap(user).expect("latest wrap missing");
        assert_eq!(latest.period, period);

        // Acceptance criteria 4: LastUpdated is set for every user in the batch.
        assert!(client.get_last_updated(user).is_some());
    }

    // Acceptance criteria 5: A mint event is emitted per item.
    let events = decode_events(&env);
    let mint_events: std::vec::Vec<_> = events
        .iter()
        .filter(|(topics, _data)| {
            if topics.is_empty() {
                return false;
            }
            if let Ok(sym) = topics[0].try_into_val(&env) {
                let s: Symbol = sym;
                // Wait, Mint events are sometimes MintEventType::Mint or symbol_short!("mint")
                // Let's check both possibilities.
                s == symbol_short!("mint") || s == Symbol::new(&env, "Mint")
            } else {
                false
            }
        })
        .collect();

    assert_eq!(mint_events.len(), 3);
}

// ─── Aggregated-signature path (issue #680) ─────────────────────────────
//
// mint_wrap_batch has two verification branches: per-item signatures (the
// happy-path test above, which passes `&None`) and a single aggregated
// signature over the whole ordered batch (`&Some(agg_sig)`). The aggregated
// branch has its own behavior not exercised through the client before now:
// it takes `payload_version` from `items.get(0)`, calls `require_auth` on
// every user before verifying anything, and commits to item order via
// `construct_batch_mint_payload`. The tests below drive that branch through
// `client.mint_wrap_batch` / `client.try_mint_wrap_batch`.

use crate::test_utils::sign_batch_payload;

#[test]
fn test_mint_wrap_batch_aggregated_signature_success() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let initial_total = client.total_wrap_count();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);

    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let payload_version = 1u32;

    // Per-item `signature` fields are irrelevant on the aggregated path (only
    // `verify_batch_aggregated_signature` runs), so leave them as dummy zeros.
    let dummy_sig = BytesN::from_array(&env, &[0u8; 64]);

    let item1 = BatchWrapItem {
        user: user1.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: dummy_sig.clone(),
    };
    let item2 = BatchWrapItem {
        user: user2.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: dummy_sig.clone(),
    };
    let item3 = BatchWrapItem {
        user: user3.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: dummy_sig,
    };

    let items = vec![&env, item1, item2, item3];
    let agg_sig = sign_batch_payload(&env, &signing_key, &contract_id, &items, payload_version);

    client.mint_wrap_batch(&items, &Some(agg_sig));

    // AC: a valid aggregated signature over N items mints all N.
    assert_eq!(client.total_wrap_count(), initial_total + 3);
    for user in [&user1, &user2, &user3] {
        assert_eq!(client.balance_of(user), 1);
        assert!(client.get_wrap(user, &period).is_some());
    }
}

#[test]
fn test_mint_wrap_batch_aggregated_signature_rejects_reordered_items() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[2u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[7u8; 32]);
    let payload_version = 1u32;
    let dummy_sig = BytesN::from_array(&env, &[0u8; 64]);

    let item1 = BatchWrapItem {
        user: user1,
        period: 202401u64,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: dummy_sig.clone(),
    };
    let item2 = BatchWrapItem {
        user: user2,
        period: 202402u64,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        signature: dummy_sig,
    };

    // Sign over [item1, item2] ...
    let signed_order = vec![&env, item1.clone(), item2.clone()];
    let agg_sig = sign_batch_payload(&env, &signing_key, &contract_id, &signed_order, payload_version);

    // ... but submit [item2, item1]. construct_batch_mint_payload commits to
    // order, so the signature no longer matches the payload.
    let reordered = vec![&env, item2, item1];

    let result = client.try_mint_wrap_batch(&reordered, &Some(agg_sig));
    assert_eq!(
        result.err().unwrap().contract_error(),
        Some(crate::ContractError::InvalidSignature as u32)
    );
}

#[test]
fn test_mint_wrap_batch_aggregated_signature_rejects_mutated_data_hash() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[3u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let payload_version = 1u32;
    let dummy_sig = BytesN::from_array(&env, &[0u8; 64]);

    let item1 = BatchWrapItem {
        user: user1,
        period: 202401u64,
        archetype: archetype.clone(),
        data_hash: BytesN::from_array(&env, &[9u8; 32]),
        payload_version,
        signature: dummy_sig.clone(),
    };
    let item2 = BatchWrapItem {
        user: user2,
        period: 202402u64,
        archetype: archetype.clone(),
        data_hash: BytesN::from_array(&env, &[9u8; 32]),
        payload_version,
        signature: dummy_sig,
    };

    let signed_items = vec![&env, item1.clone(), item2.clone()];
    let agg_sig = sign_batch_payload(&env, &signing_key, &contract_id, &signed_items, payload_version);

    // Mutate item1's data_hash after the signature was computed over it.
    let mut mutated_item1 = item1;
    mutated_item1.data_hash = BytesN::from_array(&env, &[10u8; 32]);
    let mutated_items = vec![&env, mutated_item1, item2];

    let result = client.try_mint_wrap_batch(&mutated_items, &Some(agg_sig));
    assert_eq!(
        result.err().unwrap().contract_error(),
        Some(crate::ContractError::InvalidSignature as u32)
    );
}

#[test]
fn test_mint_wrap_batch_aggregated_signature_rejects_wrong_key() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[4u8; 32]);
    let wrong_signing_key = SigningKey::from_bytes(&[5u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let user1 = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[11u8; 32]);
    let payload_version = 1u32;
    let dummy_sig = BytesN::from_array(&env, &[0u8; 64]);

    let item1 = BatchWrapItem {
        user: user1,
        period: 202401u64,
        archetype,
        data_hash,
        payload_version,
        signature: dummy_sig,
    };
    let items = vec![&env, item1];

    // Signed by a key that is not the registered admin key.
    let agg_sig = sign_batch_payload(&env, &wrong_signing_key, &contract_id, &items, payload_version);

    let result = client.try_mint_wrap_batch(&items, &Some(agg_sig));
    assert_eq!(
        result.err().unwrap().contract_error(),
        Some(crate::ContractError::InvalidSignature as u32)
    );
}

// AC: omitting `aggregated_signature` falls back to per-item signatures.
// Already covered by `test_mint_wrap_batch_happy_path` above, which calls
// `client.mint_wrap_batch(&items, &None)` and succeeds via per-item
// signatures — no separate test needed to avoid duplicating that coverage.