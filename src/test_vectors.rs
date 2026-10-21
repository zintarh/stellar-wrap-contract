#![cfg(test)]

//! Deterministic Ed25519 mint-signature fixtures (Issue #275) and
//! cross-language payload vectors (`wrap-crypto/vectors.json`).
//!
//! Fixed seeds and field values let backend/frontend/contract implementations
//! confirm they encode the same canonical mint and batch payloads.
//!
//! ## Regenerating `wrap-crypto/vectors.json`
//!
//! Prefer the contract-local ignored writer (uses the real
//! `construct_*_payload` helpers):
//!
//! ```bash
//! cargo test --lib gen_write_wrap_crypto_vectors -- --ignored --nocapture
//! ```
//!
//! If the full crate does not compile, the standalone emitter mirrors the same
//! construction rules:
//!
//! ```bash
//! cargo run --manifest-path wrap-crypto/emit/Cargo.toml
//! ```
//!
//! ## Scheme versioning (Issue #885)
//!
//! The payload carries a scheme/version discriminant (`FIXTURE_PAYLOAD_VERSION`)
//! distinct from the period and data hash. `CURRENT_PAYLOAD_VERSION` is the only
//! supported mint scheme today; unknown versions are rejected by verification.

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};
use std::path::PathBuf;
use std::string::String;
use std::vec::Vec;

use super::*;
use crate::{
    mint::CURRENT_PAYLOAD_VERSION,
    signature::{
        construct_batch_mint_payload, construct_mint_payload, verify_mint_signature,
        BATCH_MINT_SCHEME_VERSION, MINT_DOMAIN_SEPARATOR, SUPPORTED_BATCH_MINT_SCHEME_VERSIONS,
    },
    storage_types::BatchWrapItem,
};

/// Fixed Ed25519 secret-key seed (tests only).
pub const FIXTURE_SECRET_SEED: [u8; 32] = [0x42; 32];

/// Period (`YYYYMM`).
pub const FIXTURE_PERIOD: u64 = 202412;

/// Inclusive expiry bound embedded in v2 mint payloads.
pub const FIXTURE_VALID_UNTIL: u64 = u64::MAX;

/// Payload version required by `mint_wrap`.
pub const FIXTURE_PAYLOAD_VERSION: u32 = CURRENT_PAYLOAD_VERSION;

/// Non-zero data hash fixture.
pub const FIXTURE_DATA_HASH: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
];

/// Every scheme version the contract currently accepts for mint payloads.
pub const SUPPORTED_PAYLOAD_VERSIONS: [u32; 1] = [CURRENT_PAYLOAD_VERSION];

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn fixture_archetype() -> Symbol {
    symbol_short!("builder")
}

fn addr_str(addr: &Address) -> String {
    addr.to_string().to_string()
}

fn bytes_to_vec(payload: &Bytes) -> Vec<u8> {
    let len = payload.len() as usize;
    let mut buf = Vec::with_capacity(len);
    buf.resize(len, 0);
    payload.copy_into_slice(&mut buf);
    buf
}

fn vectors_json_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wrap-crypto/vectors.json")
}

fn setup_fixture_env() -> (Env, Address, Address, BytesN<32>, BytesN<64>, Bytes) {
    setup_fixture_env_for_version(FIXTURE_PAYLOAD_VERSION)
}

fn setup_fixture_env_for_version(
    version: u32,
) -> (Env, Address, Address, BytesN<32>, BytesN<64>, Bytes) {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let user = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&FIXTURE_SECRET_SEED);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    let payload = construct_mint_payload(
        &env,
        &contract_id,
        &user,
        FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        version,
        FIXTURE_VALID_UNTIL,
    );

    let buf = bytes_to_vec(&payload);
    let signature = BytesN::from_array(&env, &signing_key.sign(&buf).to_bytes());

    (env, contract_id, user, pubkey, signature, payload)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct VectorsFile {
    secret_seed_hex: String,
    pubkey_hex: String,
    mint: Vec<MintVector>,
    batch: Vec<BatchVector>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct MintVector {
    name: String,
    payload_version: u32,
    contract_id: String,
    user: String,
    period: u64,
    archetype: String,
    data_hash_hex: String,
    valid_until: String,
    payload_hex: String,
    signature_hex: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct BatchItemVector {
    user: String,
    period: u64,
    archetype: String,
    data_hash_hex: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct BatchVector {
    name: String,
    payload_version: u32,
    contract_id: String,
    items: Vec<BatchItemVector>,
    payload_hex: String,
    signature_hex: String,
}

fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Build interop vectors with `Address::generate` only (no contract register),
/// matching `wrap-crypto/emit` so the committed file stays stable across crates.
fn generate_wrap_crypto_vectors() -> VectorsFile {
    let env = Env::default();
    let contract_id = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let signing_key = SigningKey::from_bytes(&FIXTURE_SECRET_SEED);
    let pubkey = signing_key.verifying_key().to_bytes();
    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    let mut mint = Vec::new();
    for &version in SUPPORTED_PAYLOAD_VERSIONS.iter() {
        let payload = construct_mint_payload(
            &env,
            &contract_id,
            &user_a,
            FIXTURE_PERIOD,
            &archetype,
            &data_hash,
            version,
            FIXTURE_VALID_UNTIL,
        );
        let bytes = bytes_to_vec(&payload);
        let signature = signing_key.sign(&bytes).to_bytes();
        mint.push(MintVector {
            name: format!("mint_v{version}"),
            payload_version: version,
            contract_id: addr_str(&contract_id),
            user: addr_str(&user_a),
            period: FIXTURE_PERIOD,
            archetype: String::from("builder"),
            data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
            valid_until: FIXTURE_VALID_UNTIL.to_string(),
            payload_hex: hex_encode(&bytes),
            signature_hex: hex_encode(&signature),
        });
    }

    let mut batch = Vec::new();
    for &version in SUPPORTED_BATCH_MINT_SCHEME_VERSIONS.iter() {
        let mut items = soroban_sdk::Vec::new(&env);
        items.push_back(BatchWrapItem {
            user: user_a.clone(),
            period: FIXTURE_PERIOD,
            archetype: archetype.clone(),
            data_hash: data_hash.clone(),
            payload_version: version,
            valid_until: FIXTURE_VALID_UNTIL,
            signature: BytesN::from_array(&env, &[0u8; 64]),
        });
        items.push_back(BatchWrapItem {
            user: user_b.clone(),
            period: FIXTURE_PERIOD + 1,
            archetype: archetype.clone(),
            data_hash: data_hash.clone(),
            payload_version: version,
            valid_until: FIXTURE_VALID_UNTIL,
            signature: BytesN::from_array(&env, &[0u8; 64]),
        });

        let payload = construct_batch_mint_payload(&env, &contract_id, &items, version);
        let bytes = bytes_to_vec(&payload);
        let signature = signing_key.sign(&bytes).to_bytes();
        batch.push(BatchVector {
            name: format!("batch_v{version}"),
            payload_version: version,
            contract_id: addr_str(&contract_id),
            items: std::vec![
                BatchItemVector {
                    user: addr_str(&user_a),
                    period: FIXTURE_PERIOD,
                    archetype: String::from("builder"),
                    data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
                },
                BatchItemVector {
                    user: addr_str(&user_b),
                    period: FIXTURE_PERIOD + 1,
                    archetype: String::from("builder"),
                    data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
                },
            ],
            payload_hex: hex_encode(&bytes),
            signature_hex: hex_encode(&signature),
        });
    }

    VectorsFile {
        secret_seed_hex: hex_encode(&FIXTURE_SECRET_SEED),
        pubkey_hex: hex_encode(&pubkey),
        mint,
        batch,
    }
}

#[test]
fn test_deterministic_fixture_used_in_contract_mint() {
    let (env, contract_id, user, pubkey, signature, payload) = setup_fixture_env();

    let mut head = [0u8; 15];
    for (i, slot) in head.iter_mut().enumerate() {
        *slot = payload.get(i as u32).unwrap();
    }
    assert_eq!(&head, MINT_DOMAIN_SEPARATOR);

    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    assert!(verify_mint_signature(
        &env,
        &pubkey,
        &contract_id,
        &user,
        FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        FIXTURE_PAYLOAD_VERSION,
        FIXTURE_VALID_UNTIL,
        &signature,
    )
    .is_ok());

    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    env.mock_all_auths();
    client.initialize(&admin, &pubkey);

    client.mint_wrap(
        &user,
        &FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        &FIXTURE_PAYLOAD_VERSION,
        &FIXTURE_VALID_UNTIL,
        &signature,
    );
    assert!(client.get_wrap(&user, &FIXTURE_PERIOD).is_some());
}

#[test]
fn test_fixture_inputs_are_stable() {
    assert_eq!(FIXTURE_PERIOD, 202412);
    assert_eq!(FIXTURE_PAYLOAD_VERSION, 2);
    assert_ne!(FIXTURE_DATA_HASH, [0u8; 32]);
    assert_eq!(FIXTURE_SECRET_SEED, [0x42; 32]);
    assert_eq!(BATCH_MINT_SCHEME_VERSION, 1);
}

/// One deterministic vector per supported scheme version (Issue #885).
#[test]
fn test_vector_per_supported_version() {
    assert!(!SUPPORTED_PAYLOAD_VERSIONS.is_empty());
    assert!(SUPPORTED_PAYLOAD_VERSIONS.contains(&CURRENT_PAYLOAD_VERSION));

    for &version in SUPPORTED_PAYLOAD_VERSIONS.iter() {
        let (env, contract_id, user, pubkey, signature, payload) =
            setup_fixture_env_for_version(version);

        let buf = bytes_to_vec(&payload);
        let version_bytes = version.to_be_bytes();
        assert!(
            buf.windows(4).any(|w| w == version_bytes),
            "payload must embed the scheme version discriminant"
        );

        let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
        let archetype = fixture_archetype();
        assert!(verify_mint_signature(
            &env,
            &pubkey,
            &contract_id,
            &user,
            FIXTURE_PERIOD,
            &archetype,
            &data_hash,
            version,
            FIXTURE_VALID_UNTIL,
            &signature,
        )
        .is_ok());
    }
}

/// Unknown scheme versions must be rejected rather than assumed to be current.
#[test]
fn test_unknown_version_is_rejected() {
    let unknown = CURRENT_PAYLOAD_VERSION + 1;
    assert!(!SUPPORTED_PAYLOAD_VERSIONS.contains(&unknown));

    let (env, contract_id, user, pubkey, signature, _payload) =
        setup_fixture_env_for_version(unknown);
    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = fixture_archetype();

    assert!(verify_mint_signature(
        &env,
        &pubkey,
        &contract_id,
        &user,
        FIXTURE_PERIOD,
        &archetype,
        &data_hash,
        unknown,
        FIXTURE_VALID_UNTIL,
        &signature,
    )
    .is_err());
}

/// Committed `wrap-crypto/vectors.json` must match payloads produced by the
/// real Rust constructors (mint for every supported version + batch).
#[test]
fn test_wrap_crypto_vectors_json_matches_rust() {
    assert!(!SUPPORTED_PAYLOAD_VERSIONS.is_empty());
    assert!(!SUPPORTED_BATCH_MINT_SCHEME_VERSIONS.is_empty());

    let generated = generate_wrap_crypto_vectors();
    let path = vectors_json_path();
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing {}: {e}; regenerate with \
             `cargo test --lib gen_write_wrap_crypto_vectors -- --ignored` \
             or `cargo run --manifest-path wrap-crypto/emit/Cargo.toml`",
            path.display()
        )
    });
    let parsed: VectorsFile =
        serde_json::from_str(&on_disk).expect("parse wrap-crypto/vectors.json");

    assert_eq!(
        parsed, generated,
        "wrap-crypto/vectors.json drifted from Rust construct_*_payload; regenerate it"
    );

    // Re-check byte equality via the live constructors using on-disk inputs.
    let env = Env::default();
    let signing_key = SigningKey::from_bytes(&FIXTURE_SECRET_SEED);
    for v in &parsed.mint {
        let contract_id = Address::from_str(&env, &v.contract_id);
        let user = Address::from_str(&env, &v.user);
        let hash_bytes = decode_hex(&v.data_hash_hex);
        let mut hash_arr = [0u8; 32];
        hash_arr.copy_from_slice(&hash_bytes);
        let data_hash = BytesN::from_array(&env, &hash_arr);
        let archetype = Symbol::new(&env, &v.archetype);
        let valid_until: u64 = v.valid_until.parse().expect("valid_until");
        let payload = construct_mint_payload(
            &env,
            &contract_id,
            &user,
            v.period,
            &archetype,
            &data_hash,
            v.payload_version,
            valid_until,
        );
        let bytes = bytes_to_vec(&payload);
        assert_eq!(hex_encode(&bytes), v.payload_hex, "{} payload bytes", v.name);
        assert_eq!(
            hex_encode(&signing_key.sign(&bytes).to_bytes()),
            v.signature_hex,
            "{} signature",
            v.name
        );
    }

    for v in &parsed.batch {
        let contract_id = Address::from_str(&env, &v.contract_id);
        let mut items = soroban_sdk::Vec::new(&env);
        for item in &v.items {
            let user = Address::from_str(&env, &item.user);
            let hash_bytes = decode_hex(&item.data_hash_hex);
            let mut hash_arr = [0u8; 32];
            hash_arr.copy_from_slice(&hash_bytes);
            items.push_back(BatchWrapItem {
                user,
                period: item.period,
                archetype: Symbol::new(&env, &item.archetype),
                data_hash: BytesN::from_array(&env, &hash_arr),
                payload_version: v.payload_version,
                valid_until: FIXTURE_VALID_UNTIL,
                signature: BytesN::from_array(&env, &[0u8; 64]),
            });
        }
        let payload =
            construct_batch_mint_payload(&env, &contract_id, &items, v.payload_version);
        let bytes = bytes_to_vec(&payload);
        assert_eq!(hex_encode(&bytes), v.payload_hex, "{} payload bytes", v.name);
        assert_eq!(
            hex_encode(&signing_key.sign(&bytes).to_bytes()),
            v.signature_hex,
            "{} signature",
            v.name
        );
    }
}

#[test]
#[ignore]
fn gen_write_wrap_crypto_vectors() {
    let generated = generate_wrap_crypto_vectors();
    let path = vectors_json_path();
    let json = serde_json::to_string_pretty(&generated).expect("serialize vectors");
    std::fs::write(&path, format!("{json}\n")).expect("write wrap-crypto/vectors.json");
    std::println!("wrote {}", path.display());
}

#[test]
#[ignore]
fn gen_print_mint_fixtures() {
    let (_env, contract_id, user, pubkey, signature, payload) = setup_fixture_env();
    let buf = bytes_to_vec(&payload);

    std::println!("pubkey_hex={}", hex_encode(&pubkey.to_array()));
    std::println!("payload_hex={}", hex_encode(&buf));
    std::println!("signature_hex={}", hex_encode(&signature.to_array()));
    std::println!("contract={:?}", contract_id);
    std::println!("user={:?}", user);
}
