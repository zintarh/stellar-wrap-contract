//! Shared wrap-data hash vectors (RFC 8785 + SHA-256).
//!
//! These fixtures are the Rust half of `schema/vectors.json`. The TypeScript
//! reference in `scripts/wrap-data.ts` must produce the same `jcs` and `sha256`
//! for each named vector. Canonicalization happens off-chain; on-chain
//! `verify_data` only compares `sha256(data)` to the stored hash.
//!
//! Run with: `cargo test --lib data_hash_vectors`
//! Cross-check with: `cd scripts && npm run wrap-data:vectors`

#![cfg(test)]

extern crate std;

use soroban_sdk::{Bytes, Env};

/// `(name, jcs, sha256_hex)` — keep in lockstep with `schema/vectors.json`.
const VECTORS: &[(&str, &str, &str)] = &[
    (
        "minimal_score_level",
        r#"{"level":"gold","schema_version":1,"score":100}"#,
        "f7095608e4ee28315d1c25da9303d66cc17071daf3c9224b18c1630c1618522c",
    ),
    (
        "key_order_independent",
        r#"{"level":"gold","schema_version":1,"score":100}"#,
        "f7095608e4ee28315d1c25da9303d66cc17071daf3c9224b18c1630c1618522c",
    ),
    (
        "score_only",
        r#"{"schema_version":1,"score":100}"#,
        "1d1afffe4ab0b71a43bf6670a2cd6e498d797accd5553ad004a18379d6ae3ddd",
    ),
    (
        "nested_profile",
        r#"{"metadata":{"created":"2024-01-15","verified":true},"profile":{"age":30,"name":"Alice"},"schema_version":1,"scores":[100,95,87]}"#,
        "1b37ea2245158fa7f356007aba9fd1fbdb8985cf285bc9bbc9420bab1834f20a",
    ),
    (
        "number_normalization",
        r#"{"schema_version":1,"score":100}"#,
        "1d1afffe4ab0b71a43bf6670a2cd6e498d797accd5553ad004a18379d6ae3ddd",
    ),
    (
        "unicode_string",
        "{\"level\":\"caf\u{e9}\",\"schema_version\":1}",
        "4ee10cd4227e9b25a396f8d6bdedd4789c7fbddd9e7b8fc0d3c5da014cd1c746",
    ),
];

fn hex_decode(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64, "sha256 hex must be 64 chars");
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex digit");
    }
    out
}

fn hex_encode(bytes: &[u8]) -> std::string::String {
    use std::fmt::Write;
    let mut out = std::string::String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

#[test]
fn shared_vectors_match_schema_file() {
    let file = include_str!("../schema/vectors.json");
    for (name, jcs, sha256) in VECTORS {
        assert!(
            file.contains(name),
            "schema/vectors.json missing vector name {name}"
        );
        let jcs_json = jcs.replace('\\', "\\\\").replace('"', "\\\"");
        assert!(
            file.contains(&jcs_json) || file.contains(jcs),
            "schema/vectors.json missing jcs for {name}"
        );
        assert!(
            file.contains(sha256),
            "schema/vectors.json missing sha256 for {name}"
        );
    }
}

#[test]
fn shared_vectors_sha256_matches_jcs_bytes() {
    let env = Env::default();
    for (name, jcs, expected_hex) in VECTORS {
        let data = Bytes::from_slice(&env, jcs.as_bytes());
        let digest = env.crypto().sha256(&data);
        let got = hex_encode(&digest.to_array());
        assert_eq!(
            got.as_str(),
            *expected_hex,
            "vector {name}: sha256(jcs) mismatch"
        );
        assert_eq!(digest.to_array(), hex_decode(expected_hex));
    }
}

#[test]
fn key_order_vectors_share_digest() {
    let (_, jcs_a, hash_a) = VECTORS[0];
    let (_, jcs_b, hash_b) = VECTORS[1];
    assert_eq!(jcs_a, jcs_b);
    assert_eq!(hash_a, hash_b);
}
