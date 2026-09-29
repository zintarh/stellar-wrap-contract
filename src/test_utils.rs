extern crate std;

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
    let mut out = std::vec![0u8; len];
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
    use crate::StellarWrapContract;
    use crate::StellarWrapContractClient;
    use soroban_sdk::symbol_short;
    use soroban_sdk::testutils::Address as _;

    fn setup_env() -> (
        Env,
        StellarWrapContractClient<'static>,
        SigningKey,
        Address,
        Symbol,
        BytesN<32>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let signer = SigningKey::from_bytes(&[1u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signer.verifying_key().to_bytes());
        let admin = Address::generate(&env);
        let contract_id = env.register(StellarWrapContract, ());
        let client = StellarWrapContractClient::new(&env, &contract_id);
        client.initialize(&admin, &admin_pubkey);
        let user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[0u8; 32]);
        (env, client, signer, user, archetype, data_hash)
    }

    fn mint_wrap(
        env: &Env,
        client: &StellarWrapContractClient,
        signer: &SigningKey,
        user: &Address,
        period: u64,
        archetype: &Symbol,
        data_hash: &BytesN<32>,
    ) {
        let signature = sign_payload(
            env,
            signer,
            &client.address,
            user,
            period,
            archetype,
            data_hash,
        );
        client.mint_wrap(user, &period, archetype, data_hash, &1u32, &signature);
    }

    #[test]
    fn test_get_wraps_full_page_returns_all_in_insertion_order() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        let periods = [202401u64, 202403, 202402, 202405, 202404];
        for &p in &periods {
            mint_wrap(&env, &client, &signer, &user, p, &archetype, &data_hash);
        }

        let result = client.get_wraps(&user, &0, &5);
        assert_eq!(result.len(), 5);
        for (i, &p) in periods.iter().enumerate() {
            assert_eq!(result.get(i as u32).unwrap().period, p);
        }
    }

    #[test]
    fn test_get_wraps_zero_limit_returns_empty() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        mint_wrap(
            &env, &client, &signer, &user, 202401, &archetype, &data_hash,
        );

        let result = client.get_wraps(&user, &0, &0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_wraps_start_at_len_returns_empty() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        for &p in &[202401u64, 202402] {
            mint_wrap(&env, &client, &signer, &user, p, &archetype, &data_hash);
        }

        assert!(client.get_wraps(&user, &2, &5).is_empty());
        assert!(client.get_wraps(&user, &100, &5).is_empty());
    }

    #[test]
    fn test_get_wraps_start_within_len_returns_tail() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        let periods = [202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &client, &signer, &user, p, &archetype, &data_hash);
        }

        let result = client.get_wraps(&user, &3, &10);
        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0).unwrap().period, 202404);
        assert_eq!(result.get(1).unwrap().period, 202405);
    }

    #[test]
    fn test_get_wraps_limit_max_does_not_overflow() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        let periods = [202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &client, &signer, &user, p, &archetype, &data_hash);
        }

        let result = client.get_wraps(&user, &0, &u32::MAX);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_get_wraps_after_revoke_middle_period_short_page() {
        let (env, client, signer, user, archetype, data_hash) = setup_env();
        let periods = [202401u64, 202402, 202403, 202404, 202405];
        for &p in &periods {
            mint_wrap(&env, &client, &signer, &user, p, &archetype, &data_hash);
        }

        let reason = BytesN::from_array(&env, &[0u8; 32]);
        client.revoke_wrap(&user, &202403, &reason);

        let result = client.get_wraps(&user, &0, &u32::MAX);
        assert_eq!(result.len(), 4);
    }
}
