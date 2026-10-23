extern crate std;
use std::vec;

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    testutils::Events,
    xdr::{ContractEventBody, ScVal},
    Address, BytesN, Env, Symbol, TryFromVal, TryIntoVal, Val,
};

use crate::signature::{construct_batch_mint_payload, construct_mint_payload};

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

/// Every contract event recorded so far as `(contract, topics, data)`, the
/// shape `env.events().all()` returned before soroban-sdk 23.
#[allow(dead_code)]
pub(crate) fn contract_events(env: &Env) -> soroban_sdk::Vec<(Address, soroban_sdk::Vec<Val>, Val)> {
    let mut out = soroban_sdk::Vec::new(env);
    for event in env.events().all().events() {
        let Some(contract_id) = event.contract_id.clone() else {
            continue;
        };
        let contract = Address::try_from_val(
            env,
            &ScVal::Address(soroban_sdk::xdr::ScAddress::Contract(contract_id)),
        )
        .unwrap();
        let ContractEventBody::V0(body) = &event.body;
        let mut topics = soroban_sdk::Vec::new(env);
        for topic in body.topics.iter() {
            topics.push_back(topic.try_into_val(env).unwrap());
        }
        out.push_back((contract, topics, body.data.try_into_val(env).unwrap()));
    }
    out
}

/// Pulls the contract error code out of a failed `try_` call.
#[allow(dead_code)]
pub(crate) trait ContractErrorCode {
    fn contract_error(self) -> Option<u32>;
}

impl ContractErrorCode for Result<soroban_sdk::Error, soroban_sdk::InvokeError> {
    fn contract_error(self) -> Option<u32> {
        match self {
            Ok(error) if error.is_type(soroban_sdk::xdr::ScErrorType::Contract) => {
                Some(error.get_code())
            }
            Err(soroban_sdk::InvokeError::Contract(code)) => Some(code),
            _ => None,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn scval_to_val(env: &Env, scval: &ScVal) -> Val {
    scval.try_into_val(env).unwrap()
}

#[cfg(test)]
mod get_wraps_tests {
    use super::*;
    use crate::{StellarWrapContract, StellarWrapContractClient};
    use soroban_sdk::testutils::Address as _;
    use std::vec::Vec;

    // Tests use small logical periods (1, 10, 30, ...) for readability; the
    // contract only accepts YYYYMM periods, so map `p` to January of 2024 + p.
    fn real_period(p: u64) -> u64 {
        (crate::mint::MIN_PERIOD_YEAR + p) * 100 + 1
    }

    fn logical_period(period: u64) -> u64 {
        period / 100 - crate::mint::MIN_PERIOD_YEAR
    }

    fn setup_env() -> (Env, Address, SigningKey, Address, Symbol, BytesN<32>) {
        let env = Env::default();
        env.mock_all_auths();
        let signer = SigningKey::from_bytes(&[7u8; 32]);
        let contract = env.register(StellarWrapContract, ());
        let admin = Address::generate(&env);
        let pubkey = BytesN::from_array(&env, &signer.verifying_key().to_bytes());
        StellarWrapContractClient::new(&env, &contract).initialize(&admin, &pubkey);
        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, "Wrap");
        let data_hash = BytesN::from_array(&env, &[1u8; 32]);
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
        let period = real_period(period);
        let signature = sign_payload(env, signer, contract, user, period, archetype, data_hash);
        StellarWrapContractClient::new(env, contract).mint_wrap(
            user,
            &period,
            archetype,
            data_hash,
            &crate::mint::CURRENT_PAYLOAD_VERSION,
            &u64::MAX,
            &signature,
        );
    }

    fn get_wraps(env: &Env, contract: &Address, user: &Address, start: u32, limit: u32) -> Vec<u64> {
        StellarWrapContractClient::new(env, contract)
            .get_wraps(user, &start, &limit)
            .iter()
            .map(|record| logical_period(record.period))
            .collect()
    }

    fn revoke(env: &Env, contract: &Address, user: &Address, period: u64) {
        StellarWrapContractClient::new(env, contract).revoke_wrap(
            user,
            &real_period(period),
            &BytesN::from_array(env, &[9u8; 32]),
        );
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
