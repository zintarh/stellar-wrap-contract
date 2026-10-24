#![no_main]

//! Fuzz harness for `bridge_wrap_in`.
//!
//! `bridge_wrap_in` is the most adversarial entry point: its inputs come from
//! external chains and relayers. This harness drives it with arbitrary relayer
//! sets, thresholds, signature vectors, chain status and nonces, and checks:
//!
//! - every rejection is a contract error (no raw host panics);
//! - acceptance happens only when the relayer model says it should;
//! - a rejected call leaves no state behind;
//! - an accepted `(source_chain, source_nonce)` can never be processed again.

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    symbol_short,
    testutils::arbitrary::{arbitrary, Arbitrary},
    testutils::Address as _,
    xdr::ScErrorType,
    Address, BytesN, Env, Vec,
};
use stellar_wrap_contract::{
    signature::{construct_inbound_bridge_payload, INBOUND_BRIDGE_SCHEME_VERSION},
    StellarWrapContract, StellarWrapContractClient, MAX_PERIOD_YEAR, MIN_PERIOD_YEAR,
};

/// Upper bound on relayers so the harness stays fast.
const MAX_RELAYERS: usize = 4;
/// Upper bound on submitted signatures per call.
const MAX_SIGNATURES: usize = 6;

/// One signature in the submitted vector.
#[derive(Clone, Debug, Arbitrary)]
pub struct SignatureInput {
    /// Index into the configured relayer set. Used when `valid` is true.
    pub relayer: u8,
    /// When true, sign the canonical inbound payload with that relayer's key.
    pub valid: bool,
    /// Raw signature bytes used when `valid` is false.
    pub rogue_signature: [u8; 64],
}

/// Structured fuzz input for `bridge_wrap_in`.
#[derive(Clone, Debug, Arbitrary)]
pub struct BridgeWrapInInput {
    /// Source chain ID. Zero is never a supported chain.
    pub source_chain: u32,
    /// Source-chain nonce that must be processed at most once.
    pub source_nonce: u64,
    /// Candidate period (`YYYYMM` when valid).
    pub period: u64,
    /// Wrap data hash bound into the signed payload.
    pub data_hash: [u8; 32],
    /// Seeds for the relayer keys. Only the first `relayer_count` are used.
    pub relayer_seeds: [[u8; 32]; MAX_RELAYERS],
    /// Number of relayers to register (taken modulo `MAX_RELAYERS + 1`).
    pub relayer_count: u8,
    /// Threshold passed to `set_bridge_relayers`.
    pub threshold: u32,
    /// Whether the source chain is enabled before the call.
    pub chain_enabled: bool,
    /// Signature vector submitted to `bridge_wrap_in`.
    pub signatures: std::vec::Vec<SignatureInput>,
    /// When true, replay the exact same call after a successful acceptance.
    pub replay: bool,
}

fn period_is_valid(period: u64) -> bool {
    let year = period / 100;
    let month = period % 100;
    (MIN_PERIOD_YEAR..=MAX_PERIOD_YEAR).contains(&year) && (1..=12).contains(&month)
}

fuzz_target!(|input: BridgeWrapInInput| {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(
        &env,
        &SigningKey::from_bytes(&[7u8; 32]).verifying_key().to_bytes(),
    );
    client.initialize(&admin, &admin_pubkey);

    let relayer_count = (input.relayer_count as usize) % (MAX_RELAYERS + 1);
    let signing_keys: std::vec::Vec<SigningKey> = input.relayer_seeds[..relayer_count]
        .iter()
        .map(SigningKey::from_bytes)
        .collect();
    let pubkeys: std::vec::Vec<[u8; 32]> = signing_keys
        .iter()
        .map(|key| key.verifying_key().to_bytes())
        .collect();

    // Model of `set_bridge_relayers`: threshold must be in 1..=len and the
    // relayer keys must be pairwise distinct.
    let distinct = pubkeys
        .iter()
        .enumerate()
        .all(|(i, a)| pubkeys[..i].iter().all(|b| a != b));
    let setup_ok = input.threshold >= 1
        && (input.threshold as usize) <= relayer_count
        && distinct;

    let chain = input.source_chain;
    let relayer_vec: Vec<BytesN<32>> = {
        let mut v = Vec::new(&env);
        for pk in &pubkeys {
            v.push_back(BytesN::from_array(&env, pk));
        }
        v
    };
    let setup = client.try_set_bridge_relayers(&chain, &relayer_vec, &input.threshold);
    assert_eq!(
        matches!(setup, Ok(Ok(()))),
        setup_ok,
        "set_bridge_relayers acceptance diverged from model"
    );

    if input.chain_enabled {
        // Chain 0 is rejected by the contract; the call result is irrelevant here.
        let _ = client.try_set_chain_status(&chain, &true);
    }
    let chain_ok = input.chain_enabled && chain != 0;

    let recipient = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &input.data_hash);

    // Build the submitted signature vector and the model's view of it. Each
    // valid signature must match a distinct relayer, and any non-matching
    // signature fails the whole call.
    let submitted = &input.signatures[..input.signatures.len().min(MAX_SIGNATURES)];
    let mut signatures: Vec<BytesN<64>> = Vec::new(&env);
    let mut used = std::vec![false; relayer_count];
    // Every submitted signature must be valid and map to a distinct relayer,
    // and there must be at least `threshold` of them.
    let mut sigs_ok = submitted.len() >= input.threshold as usize;
    for sig in submitted {
        let bytes = if sig.valid && relayer_count > 0 {
            let idx = (sig.relayer as usize) % relayer_count;
            if used[idx] {
                sigs_ok = false;
            }
            used[idx] = true;

            let payload = construct_inbound_bridge_payload(
                &env,
                &contract_id,
                chain,
                input.source_nonce,
                &recipient,
                input.period,
                &archetype,
                &data_hash,
                INBOUND_BRIDGE_SCHEME_VERSION,
            );
            let mut out = std::vec![0u8; payload.len() as usize];
            payload.copy_into_slice(&mut out);
            signing_keys[idx].sign(&out).to_bytes()
        } else {
            sigs_ok = false;
            sig.rogue_signature
        };
        signatures.push_back(BytesN::from_array(&env, &bytes));
    }
    let expected_ok = setup_ok && chain_ok && sigs_ok && period_is_valid(input.period);

    let result = client.try_bridge_wrap_in(
        &chain,
        &input.source_nonce,
        &recipient,
        &input.period,
        &archetype,
        &data_hash,
        &signatures,
    );

    match &result {
        Ok(Ok(())) => {
            assert!(
                expected_ok,
                "bridge_wrap_in accepted input the relayer model rejects"
            );
            assert!(
                client.is_inbound_nonce_processed(&chain, &input.source_nonce),
                "accepted nonce must be marked processed"
            );
            let record = client
                .get_inbound_bridge_record(&chain, &input.source_nonce)
                .expect("accepted nonce must persist an inbound record");
            assert_eq!(record.source_chain, chain);
            assert_eq!(record.source_nonce, input.source_nonce);
            assert_eq!(record.recipient, recipient);
            assert_eq!(record.period, input.period);
            assert_eq!(record.data_hash, data_hash);
            assert!(
                client.get_wrap(&recipient, &input.period).is_some(),
                "accepted bridge transfer must persist a wrap"
            );
            assert_eq!(client.balance_of(&recipient), 1);

            if input.replay {
                // Same (source_chain, source_nonce) must never be processed twice.
                let replay = client.try_bridge_wrap_in(
                    &chain,
                    &input.source_nonce,
                    &recipient,
                    &input.period,
                    &archetype,
                    &data_hash,
                    &signatures,
                );
                assert!(
                    !matches!(replay, Ok(Ok(()))),
                    "replay of an accepted (source_chain, source_nonce) must fail"
                );
                assert_eq!(
                    client.balance_of(&recipient),
                    1,
                    "failed replay must not change wrap count"
                );
            }
        }
        Err(Ok(err)) => {
            assert!(
                err.is_type(ScErrorType::Contract),
                "bridge_wrap_in must only fail with contract errors, got {err:?}"
            );
            assert!(
                !expected_ok,
                "valid bridge input was rejected with {err:?}"
            );
            // Rejected calls must be atomic.
            assert!(
                !client.is_inbound_nonce_processed(&chain, &input.source_nonce),
                "rejected bridge call must not mark the nonce processed"
            );
            assert!(
                client
                    .get_inbound_bridge_record(&chain, &input.source_nonce)
                    .is_none(),
                "rejected bridge call must not persist an inbound record"
            );
            assert!(
                client.get_wrap(&recipient, &input.period).is_none(),
                "rejected bridge call must not persist a wrap"
            );
            assert_eq!(client.balance_of(&recipient), 0);
        }
        Ok(Err(err)) => {
            panic!("bridge_wrap_in returned an unconvertible value: {err:?}");
        }
        Err(Err(err)) => {
            panic!("bridge_wrap_in returned an invocation error: {err:?}");
        }
    }
});
