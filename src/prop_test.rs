//! Property-based tests for the StellarWrap contract.

#![cfg(test)]
extern crate std;

use super::*;

use ed25519_dalek::{Signer, SigningKey};
use proptest::prelude::*;
use soroban_sdk::{
    testutils::Address as _,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Symbol,
};

// ── Shared test constants ────────────────────────────────────────────────────

const TEST_SIGNING_KEY_BYTES: [u8; 32] = [0xAB; 32];

// ── Helper: allowed archetypes ────────────────────────────────────────────────

fn allowed_archetype_symbols() -> Vec<&'static str> {
    vec!["builder", "arch", "architect", "soroban", "defi", "patron"]
}

// ── Helper: environment setup ─────────────────────────────────────────────────

fn setup_env() -> (
    Env,
    StellarWrapContractClient<'static>,
    Address,
    SigningKey,
    [u8; 32],
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&TEST_SIGNING_KEY_BYTES);
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    let admin_pubkey = BytesN::from_array(&env, &pubkey_bytes);
    let admin = Address::generate(&env);

    client.initialize(&admin, &admin_pubkey);

    (env, client, contract_id, signing_key, pubkey_bytes)
}

// ── Helper: build a valid non-zero data hash ─────────────────────────────────

fn make_data_hash(env: &Env, raw: [u8; 32]) -> BytesN<32> {
    if raw == [0u8; 32] {
        BytesN::from_array(env, &[0x01u8; 32])
    } else {
        BytesN::from_array(env, &raw)
    }
}

// ── Helper: sign a canonical mint payload ────────────────────────────────────

fn sign_mint(
    env: &Env,
    signing_key: &SigningKey,
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

    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);

    let sig = signing_key.sign(&buf[..len]);
    BytesN::from_array(env, &sig.to_bytes())
}

// ── Proptest strategies ───────────────────────────────────────────────────────

fn arb_period() -> impl Strategy<Value = u64> {
    1u64..=u64::MAX
}

fn arb_data_hash() -> impl Strategy<Value = [u8; 32]> {
    any::<[u8; 32]>().prop_map(|b| {
        if b == [0u8; 32] { [0x01u8; 32] } else { b }
    })
}

fn arb_archetype() -> impl Strategy<Value = &'static str> {
    prop::sample::select(allowed_archetype_symbols())
}

// ── Property Tests ───────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_valid_mint_is_retrievable(
        period in arb_period(),
        raw_hash in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let data_hash = make_data_hash(&env, raw_hash);
        let archetype = Symbol::new(&env, archetype_str);

        let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash);

        client.mint_wrap(&user, &period, &archetype, &data_hash, &sig);

        let record = client.get_wrap(&user, &period)
            .expect("get_wrap must return Some after successful mint");

        prop_assert_eq!(record.data_hash, data_hash);
        prop_assert_eq!(record.archetype, archetype);
        prop_assert_eq!(record.period, period);
    }
}

proptest! {
    #[test]
    fn prop_balance_equals_mint_count(
        periods_raw in proptest::collection::hash_set(arb_period(), 1..=8usize),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        let mut periods: Vec<u64> = periods_raw.into_iter().collect();
        periods.sort_unstable();

        for (k, &period) in periods.iter().enumerate() {
            let data_hash = make_data_hash(&env, [(k as u8).wrapping_add(1); 32]);

            let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash);

            client.mint_wrap(&user, &period, &archetype, &data_hash, &sig);

            let expected_balance = (k as u32) + 1;
            prop_assert_eq!(client.balance_of(&user), expected_balance);
        }
    }
}

proptest! {
    #[test]
    fn prop_duplicate_period_always_fails(
        period in arb_period(),
        raw_hash_first in arb_data_hash(),
        raw_hash_second in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        let data_hash_first = make_data_hash(&env, raw_hash_first);
        let sig_first = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash_first);
        client.mint_wrap(&user, &period, &archetype, &data_hash_first, &sig_first);

        let data_hash_second = make_data_hash(&env, raw_hash_second);
        let sig_second = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash_second);

        let result = client.try_mint_wrap(&user, &period, &archetype, &data_hash_second, &sig_second);
        prop_assert!(result.is_err());

        let stored = client.get_wrap(&user, &period).expect("original wrap must still exist");
        prop_assert_eq!(stored.data_hash, data_hash_first);
    }
}

proptest! {
    #[test]
    fn prop_zero_data_hash_always_rejected(
        period in arb_period(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

        let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &zero_hash);

        let result = client.try_mint_wrap(&user, &period, &archetype, &zero_hash, &sig);
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_balance_is_monotonically_increasing(
        periods_raw in proptest::collection::hash_set(arb_period(), 2..=6usize),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        let mut periods: Vec<u64> = periods_raw.into_iter().collect();
        periods.sort_unstable();

        let mut prev_balance: u32 = 0;

        for (k, &period) in periods.iter().enumerate() {
            let data_hash = make_data_hash(&env, [(k as u8).wrapping_add(2); 32]);
            let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash);

            client.mint_wrap(&user, &period, &archetype, &data_hash, &sig);

            let new_balance = client.balance_of(&user);
            prop_assert!(new_balance > prev_balance);
            prev_balance = new_balance;
        }
    }
}

proptest! {
    #[test]
    fn prop_get_wrap_returns_none_for_unminted_period(
        period in arb_period(),
    ) {
        let (env, client, _, _, _) = setup_env();
        let user = Address::generate(&env);

        prop_assert!(client.get_wrap(&user, &period).is_none());
    }
}