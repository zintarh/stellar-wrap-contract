Viewed bridge_test.rs:1-480

Here is the complete, resolved code to copy and paste into **`stellar-wrap-contract/src/bridge_test.rs`**:

```rust
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
    let contract_id = env.register(StellarWrapContract, ());
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
fn test_mint_wrap_and_bridge_wrap_in_period_validation_parity() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, signing_key) = setup_test_env(&env);
    client.set_bridge_relayer(&relayer);
    let chain_id = 1u32;
    client.set_chain_status(&chain_id, &true);

    let test_cases = [
        // Valid periods (MIN_PERIOD_YEAR = 2024, MAX_PERIOD_YEAR = 2100)
        (202401u64, true),
        (202412u64, true),
        (205006u64, true),
        (210001u64, true),
        (210012u64, true),
        // Invalid periods
        (0u64, false),
        (202312u64, false), // Below MIN_PERIOD_YEAR
        (210101u64, false), // Above MAX_PERIOD_YEAR
        (202400u64, false), // Month 0
        (202413u64, false), // Month 13
        (210000u64, false), // Month 0 in max year
        (210013u64, false), // Month 13 in max year
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
        let bridge_result = catch_unwind(AssertUnwindSafe(|| {
            client.bridge_wrap_in(&chain_id, &nonce, &bridge_user, &period, &archetype, &data_hash);
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
fn test_bridge_wrap_in_mint_and_transfer_invariants() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, signing_key) = setup_test_env(&env);
    client.set_bridge_relayer(&relayer);
    let source_chain = 1u32;
    client.set_chain_status(&source_chain, &true);

    let recipient = Address::generate(&env);
    let other_user = Address::generate(&env);
    let period1 = 202607u64;
    let period2 = 202608u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[99u8; 32]);

    // 1. bridge_wrap_in for a fresh recipient
    client.bridge_wrap_in(
        &source_chain,
        &1u64,
        &recipient,
        &period1,
        &archetype,
        &data_hash,
    );

    // Verify index invariant after bridge-in: WrapCount == WrapPeriods.len() == UserPeriods.len()
    env.as_contract(&client.address, || {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::WrapCount(recipient.clone()))
            .unwrap();
        let user_periods: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserPeriods(recipient.clone()))
            .unwrap();
        let wrap_periods: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WrapPeriods(recipient.clone()))
            .expect("WrapPeriods must exist after bridge_wrap_in");

        assert_eq!(count as usize, wrap_periods.len() as usize);
        assert_eq!(wrap_periods.len() as usize, user_periods.len() as usize);
    });

    // 2. mint_wrap for a different period for the same recipient succeeds
    let sig2 = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &recipient,
        period2,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&recipient, &period2, &archetype, &data_hash, &1, &sig2);

    // 3. transfer_wrap of the bridged-in record succeeds
    client.transfer_wrap(&recipient, &other_user, &period1);

    // 4. bridge_wrap_in for an existing period updates rather than duplicating the index entry
    let recipient_bytes = Bytes::from_array(&env, b"dest");
    let _nonce = client.bridge_wrap_out(&recipient, &source_chain, &recipient_bytes, &period2);

    client.bridge_wrap_in(
        &source_chain,
        &2u64,
        &recipient,
        &period2,
        &archetype,
        &data_hash,
    );

    env.as_contract(&client.address, || {
        let final_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::WrapCount(recipient.clone()))
            .unwrap();
        let final_user_periods: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserPeriods(recipient.clone()))
            .unwrap();
        let final_wrap_periods: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WrapPeriods(recipient.clone()))
            .unwrap();

        assert_eq!(final_count, 1);
        assert_eq!(final_wrap_periods.len(), 1);
        assert_eq!(final_user_periods.len(), 2);
    });
}
```