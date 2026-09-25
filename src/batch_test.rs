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
