extern crate std;
use std::vec;

use std::vec;

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    testutils::Events,
    xdr::{ContractEventBody, ScVal},
    Address, BytesN, Env, Symbol, TryIntoVal, Val,
};

use crate::signature::{construct_batch_mint_payload, construct_mint_payload};

/// Signs the same payload layout the contract rebuilds in `mint::mint_wrap`.
#[allow(dead_code)]
pub(crate) fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    sign_payload_versioned(env, signer, contract, user, period, archetype, data_hash, 1)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sign_payload_versioned(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> BytesN<64> {
    let payload = construct_mint_payload(
        env,
        contract,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
    );
    let len = payload.len() as usize;
    let mut out = vec![0u8; len];
    payload.copy_into_slice(&mut out);

    let signature = signer.sign(&out);
    BytesN::from_array(env, &signature.to_bytes())
}

/// Signs the aggregated batch payload that `verify_batch_aggregated_signature` verifies.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn sign_batch_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    items: &soroban_sdk::Vec<crate::storage_types::BatchWrapItem>,
    payload_version: u32,
) -> BytesN<64> {
    let payload = construct_batch_mint_payload(env, contract, items, payload_version);
    let len = payload.len() as usize;
    let mut out = std::vec![0u8; len];
    payload.copy_into_slice(&mut out);
    let signature = signer.sign(&out);
    BytesN::from_array(env, &signature.to_bytes())
}

/// Decodes emitted events into `(topics, data)` pairs of `Val`s.
///
/// Soroban SDK 27 exposes events as XDR (`ContractEvent`/`ScVal`), so tests
/// convert each topic and the data payload into a `Val` via `TryIntoVal` and
/// then decode into the expected types as before.
#[allow(dead_code)]
pub(crate) fn decode_events(env: &Env) -> std::vec::Vec<(std::vec::Vec<Val>, Val)> {
    env.events()
        .all()
        .events()
        .iter()
        .map(|event| match &event.body {
            ContractEventBody::V0(body) => {
                let topics: std::vec::Vec<Val> = body
                    .topics
                    .iter()
                    .map(|t| t.try_into_val(env).unwrap())
                    .collect();
                let data: Val = body.data.try_into_val(env).unwrap();
                (topics, data)
            },
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn scval_to_val(env: &Env, scval: &ScVal) -> Val {
    scval.try_into_val(env).unwrap()
}

#[cfg(test)]
mod get_wraps_tests {
    use super::*;
    use crate::contract::Wrap;
    use std::vec::Vec;

    fn setup_env() -> (Env, Address, SigningKey, Address, Symbol, BytesN<32>) {
        let env = Env::default();
        env.mock_all_auths();
        let signer = SigningKey::from_bytes(&[0u8; 32]);
        let contract = env.register_contract(None, Wrap);
        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, "Wrap");
        let data_hash = BytesN::from_array(&env, &[0u8; 32]);
        (env, contract, signer, user, archetype, data_hash)
    }

    fn mint_wrap(
        env: &Env,
        contract: &Address,
        signer: &SigningKey,
        user: &Address,
        period: u64,
        archetype: &Symbol,
        data_hash: &BytesN<32>,
    ) {
        let signature = sign_payload(env, signer, contract, user, period, archetype, data_hash);
        env.invoke_contract::<()>(
            contract,
            "mint_wrap",
            (user.clone(), period, archetype.clone(), data_hash.clone(), signature),
        );
    }

    fn get_wraps(env: &Env, contract: &Address, user: &Address, start: u32, limit: u32) -> Vec<u64> {
        env.invoke_contract(contract, "get_wraps", (user.clone(), start, limit))
    }

    fn revoke(env: &Env, contract: &Address, user: &Address, period: u64) {
        env.invoke_contract::<()>(contract, "revoke", (user.clone(), period));
    }

    #[test]
    fn test_get_wraps_full_page_returns_all_in_insertion_order() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![10u64, 30, 20, 50, 40];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 0, 5);
        assert_eq!(result, periods);
    }

    #[test]
    fn test_get_wraps_zero_limit_returns_empty() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        mint_wrap(&env, &contract, &signer, &user, 1, &archetype, &data_hash);

        let result = get_wraps(&env, &contract, &user, 0, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_wraps_start_at_len_returns_empty() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        for &p in &[1u64, 2] {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        assert!(get_wraps(&env, &contract, &user, 2, 5).is_empty());
        assert!(get_wraps(&env, &contract, &user, 100, 5).is_empty());
    }

    #[test]
    fn test_get_wraps_start_within_len_returns_tail() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![10u64, 20, 30, 40, 50];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 3, 10);
        assert_eq!(result, vec![40, 50]);
    }

    #[test]
    fn test_get_wraps_limit_max_does_not_overflow() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![1u64, 2, 3, 4, 5];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 0, u32::MAX);
        assert_eq!(result, periods);
    }

    #[test]
    fn test_get_wraps_after_revoke_middle_period_short_page() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![10u64, 20, 30, 40, 50];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        // Revoke a middle period (e.g., 30)
        revoke(&env, &contract, &user, 30);

        let result = get_wraps(&env, &contract, &user, 0, u32::MAX);
        assert_eq!(result, vec![10, 20, 40, 50]);
    }
}
