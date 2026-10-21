//! Emit `wrap-crypto/vectors.json` from the same payload construction rules as
//! `src/signature.rs` (`construct_mint_payload` / `construct_batch_mint_payload`).
//!
//! This binary exists so vectors can be regenerated without compiling the full
//! contract crate. Keep the construction below byte-identical to `signature.rs`.
//!
//! ```bash
//! cargo run --manifest-path wrap-crypto/emit/Cargo.toml
//! ```

use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use soroban_sdk::{
    contracttype, symbol_short, testutils::Address as _, xdr::ToXdr, Address, Bytes, BytesN, Env,
    Symbol,
};
use std::path::PathBuf;

const MINT_DOMAIN_SEPARATOR: &[u8; 15] = b"stellar-wrap-v1";
const BATCH_MINT_DOMAIN_SEPARATOR: &[u8; 21] = b"stellar-wrap-batch-v1";

const FIXTURE_SECRET_SEED: [u8; 32] = [0x42; 32];
const FIXTURE_PERIOD: u64 = 202412;
const FIXTURE_VALID_UNTIL: u64 = u64::MAX;
const FIXTURE_DATA_HASH: [u8; 32] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
];

/// Mint scheme versions covered by interop vectors (must stay aligned with
/// `CURRENT_PAYLOAD_VERSION` / migration window in the contract).
const MINT_PAYLOAD_VERSIONS: &[u32] = &[2];
/// Batch scheme versions covered by interop vectors.
const BATCH_PAYLOAD_VERSIONS: &[u32] = &[1];

#[contracttype]
#[derive(Clone)]
struct MintPayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub period: u64,
    pub user: Address,
    pub valid_until: u64,
}

fn construct_mint_payload(
    e: &Env,
    contract_id: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
    valid_until: u64,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, MINT_DOMAIN_SEPARATOR));
    let typed_payload = MintPayload {
        archetype: archetype.clone(),
        contract_id: contract_id.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        period,
        user: user.clone(),
        valid_until,
    };
    payload.append(&typed_payload.to_xdr(e));
    payload
}

fn construct_batch_mint_payload(
    e: &Env,
    contract_id: &Address,
    users: &[Address],
    periods: &[u64],
    archetypes: &[Symbol],
    data_hashes: &[BytesN<32>],
    payload_version: u32,
) -> Bytes {
    assert_eq!(users.len(), periods.len());
    assert_eq!(users.len(), archetypes.len());
    assert_eq!(users.len(), data_hashes.len());

    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, BATCH_MINT_DOMAIN_SEPARATOR));
    payload.append(&payload_version.to_xdr(e));
    payload.append(&contract_id.clone().to_xdr(e));
    let len = users.len() as u32;
    payload.append(&len.to_xdr(e));
    for i in 0..users.len() {
        payload.append(&users[i].clone().to_xdr(e));
        payload.append(&periods[i].to_xdr(e));
        payload.append(&archetypes[i].clone().to_xdr(e));
        payload.append(&data_hashes[i].clone().to_xdr(e));
    }
    payload
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn bytes_to_vec(payload: &Bytes) -> Vec<u8> {
    let len = payload.len() as usize;
    let mut buf = vec![0u8; len];
    payload.copy_into_slice(&mut buf);
    buf
}

fn addr_str(addr: &Address) -> String {
    addr.to_string().to_string()
}

#[derive(Serialize)]
struct VectorsFile {
    secret_seed_hex: String,
    pubkey_hex: String,
    mint: Vec<MintVector>,
    batch: Vec<BatchVector>,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
struct BatchItemVector {
    user: String,
    period: u64,
    archetype: String,
    data_hash_hex: String,
}

#[derive(Serialize)]
struct BatchVector {
    name: String,
    payload_version: u32,
    contract_id: String,
    items: Vec<BatchItemVector>,
    payload_hex: String,
    signature_hex: String,
}

fn main() {
    let env = Env::default();
    // Deterministic addresses (Env::default seed). Do not `register` a contract:
    // WASM hash would couple vectors to the full contract crate.
    let contract_id = Address::generate(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    let signing_key = SigningKey::from_bytes(&FIXTURE_SECRET_SEED);
    let pubkey = signing_key.verifying_key().to_bytes();
    let data_hash = BytesN::from_array(&env, &FIXTURE_DATA_HASH);
    let archetype = symbol_short!("builder");

    let mut mint = Vec::new();
    for &version in MINT_PAYLOAD_VERSIONS {
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
            archetype: "builder".into(),
            data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
            valid_until: FIXTURE_VALID_UNTIL.to_string(),
            payload_hex: hex_encode(&bytes),
            signature_hex: hex_encode(&signature),
        });
    }

    let mut batch = Vec::new();
    for &version in BATCH_PAYLOAD_VERSIONS {
        let users = [user_a.clone(), user_b.clone()];
        let periods = [FIXTURE_PERIOD, FIXTURE_PERIOD + 1];
        let archetypes = [archetype.clone(), archetype.clone()];
        let hashes = [data_hash.clone(), data_hash.clone()];
        let payload = construct_batch_mint_payload(
            &env,
            &contract_id,
            &users,
            &periods,
            &archetypes,
            &hashes,
            version,
        );
        let bytes = bytes_to_vec(&payload);
        let signature = signing_key.sign(&bytes).to_bytes();
        batch.push(BatchVector {
            name: format!("batch_v{version}"),
            payload_version: version,
            contract_id: addr_str(&contract_id),
            items: vec![
                BatchItemVector {
                    user: addr_str(&user_a),
                    period: periods[0],
                    archetype: "builder".into(),
                    data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
                },
                BatchItemVector {
                    user: addr_str(&user_b),
                    period: periods[1],
                    archetype: "builder".into(),
                    data_hash_hex: hex_encode(&FIXTURE_DATA_HASH),
                },
            ],
            payload_hex: hex_encode(&bytes),
            signature_hex: hex_encode(&signature),
        });
    }

    let file = VectorsFile {
        secret_seed_hex: hex_encode(&FIXTURE_SECRET_SEED),
        pubkey_hex: hex_encode(&pubkey),
        mint,
        batch,
    };

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vectors.json");
    let json = serde_json::to_string_pretty(&file).expect("serialize vectors");
    std::fs::write(&out, format!("{json}\n")).expect("write vectors.json");
    println!("wrote {}", out.display());
}
