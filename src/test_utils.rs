#![cfg(test)]
#![allow(dead_code)]

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::testutils::Events;
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, BytesN, Env, Symbol, TryIntoVal, Val};

use crate::signature::construct_mint_payload;

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

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
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
            }
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn scval_to_val(env: &Env, scval: &ScVal) -> Val {
    scval.try_into_val(env).unwrap()
}
