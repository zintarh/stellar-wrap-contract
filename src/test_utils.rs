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

/// A ledger timestamp (Unix seconds, UTC) used as the default in tests.
///
/// Set to 2025-06-15 00:00:00 UTC so that every valid period from
/// 202401 through 210012 is in the "past" relative to the ledger,
/// letting existing tests mint without hitting `PeriodInFuture`.
/// Tests that specifically validate future-period rejection must
/// override this with a past timestamp.
pub(crate) const TEST_LEDGER_TIMESTAMP: u64 = 4_133_980_800; // 2100-12-31 + 1 second

/// Returns an `Env::default()` pre-configured with `TEST_LEDGER_TIMESTAMP`.
///
/// All new tests should prefer this over bare `Env::default()` so they
/// automatically satisfy the future-period guard added in issue #1034.
#[allow(dead_code)]
pub(crate) fn default_test_env() -> Env {
    let env = Env::default();
    env.ledger().set_timestamp(TEST_LEDGER_TIMESTAMP);
    env
}

/// Signs the same payload layout the contract rebuilds in `mint::mint_wrap`.
/// Uses payload version 2 and sets valid_until to u64::MAX (never expires).
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
    sign_payload_versioned(env, signer, contract, user, period, archetype, data_hash, 2, u64::MAX)
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
    valid_until: u64,
) -> BytesN<64> {
    let payload = construct_mint_payload(
        env,
        contract,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
        valid_until,
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
        let env = default_test_env();
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
            (user.clone(), period, archetype.clone(), data_hash.clone(), crate::mint::CURRENT_PAYLOAD_VERSION, u64::MAX, signature),
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
        let periods = vec![202401u64, 202403, 202402, 202405, 202404];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 0, 5);
        assert_eq!(result, periods);
    }

    #[test]
    fn test_get_wraps_zero_limit_returns_empty() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        mint_wrap(&env, &contract, &signer, &user, 202401, &archetype, &data_hash);

        let result = get_wraps(&env, &contract, &user, 0, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_wraps_start_at_len_returns_empty() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        for &p in &[202401u64, 202402] {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        assert!(get_wraps(&env, &contract, &user, 2, 5).is_empty());
        assert!(get_wraps(&env, &contract, &user, 100, 5).is_empty());
    }

    #[test]
    fn test_get_wraps_start_within_len_returns_tail() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 3, 10);
        assert_eq!(result, vec![202404, 202405]);
    }

    #[test]
    fn test_get_wraps_limit_max_does_not_overflow() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        let result = get_wraps(&env, &contract, &user, 0, u32::MAX);
        assert_eq!(result, periods);
    }

    #[test]
    fn test_get_wraps_after_revoke_middle_period_short_page() {
        let (env, contract, signer, user, archetype, data_hash) = setup_env();
        let periods = vec![202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &contract, &signer, &user, p, &archetype, &data_hash);
        }

        // Revoke a middle period (e.g., 202403)
        revoke(&env, &contract, &user, 202403);

        let result = get_wraps(&env, &contract, &user, 0, u32::MAX);
        assert_eq!(result, vec![202401, 202402, 202404, 202405]);
    }
}
