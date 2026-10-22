# Using `verify_data` for Off-Chain JSON Integrity Checks

`verify_data` lets you confirm that raw data bytes match the SHA-256 hash
committed to the chain at mint time. This guide defines the **canonical wrap
JSON** rules so the frontend, signer, and any third-party verifier hash the
same bytes.

---

## How it works

When a wrap is minted the caller supplies a `data_hash: BytesN<32>`, which is
the SHA-256 digest of the raw data bytes they want to commit. The contract
stores that hash inside the `WrapRecord`.

`verify_data` accepts the original raw bytes and recomputes the hash on-chain
(see `src/queries.rs`):

```
stored_hash = WrapRecord.data_hash          (set at mint time)
computed    = sha256(data)                  (computed from the bytes you pass in)
result      = stored_hash == computed
```

It returns `true` only when the bytes you supply hash to exactly the value
stored in the record. If no wrap exists for the given `(user, period)` it
returns `false` without error.

The contract does **not** parse JSON. Canonicalization is entirely off-chain.

---

## Canonical wrap JSON (RFC 8785)

All parties MUST produce `data_hash` as:

```
data_hash = SHA-256( UTF-8( JCS(wrap_object) ) )
```

where `JCS` is the JSON Canonicalization Scheme
([RFC 8785](https://www.rfc-editor.org/rfc/rfc8785)).

RFC 8785 removes ambiguity around:

- object key order (keys are sorted)
- insignificant whitespace
- number formatting (ECMAScript `NumberToString`)

Pretty-printed JSON, reordered keys, or alternate number spellings (`100.0`
vs `100`) MUST NOT be hashed directly — canonicalize first.

### Schema version inside the JSON

Every wrap JSON document MUST include a `schema_version` field. The current
value is `1`, defined by [`schema/wrap-data.schema.json`](../schema/wrap-data.schema.json).

This is **not** the on-chain storage `schema_version()` query. It versions the
off-chain wrap payload shape so verifiers know which field rules apply.

Example (logical object; key order here is illustrative only):

```json
{
  "schema_version": 1,
  "score": 100,
  "level": "gold"
}
```

RFC 8785 canonical form of that object (what you actually hash):

```text
{"level":"gold","schema_version":1,"score":100}
```

Unknown `schema_version` values MUST be rejected by off-chain verifiers.

### Reference implementation and vectors

| Artifact | Path |
|----------|------|
| JSON Schema | [`schema/wrap-data.schema.json`](../schema/wrap-data.schema.json) |
| Shared vectors | [`schema/vectors.json`](../schema/vectors.json) |
| TypeScript helpers (`canonicalize` + `sha256`) | [`scripts/wrap-data.ts`](../scripts/wrap-data.ts) |
| Rust tests (same digests) | [`src/data_hash_vectors_test.rs`](../src/data_hash_vectors_test.rs) |

Verify the shared vectors from the TypeScript side:

```bash
cd scripts && npm install && npm run wrap-data:vectors
```

Rust:

```bash
cargo test --lib data_hash_vectors
```

Acceptance: every vector’s `jcs` and `sha256` match between TS and Rust.

---

## Preparing the input bytes

1. Build a wrap object that includes `schema_version` (and any app fields).
2. Canonicalize with RFC 8785 → UTF-8 bytes (no trailing newline).
3. Compute `sha256(bytes)` → `data_hash` for `mint_wrap`.
4. Pass the **same** canonical bytes to `verify_data`.

> **⚠ Non-canonical bytes will fail.**
>
> SHA-256 is a byte-for-byte hash. Common pitfalls if you skip JCS:
>
> - `{"score":100}` and `{ "score": 100 }` are different byte sequences.
> - `{"a":1,"b":2}` and `{"b":2,"a":1}` are different byte sequences.
> - A trailing newline (`\n`) changes the hash.
> - A BOM (byte-order mark) at the start of a file changes the hash.

TypeScript (reference):

```ts
import { dataHashOf, canonicalizeWrapData } from "./wrap-data";

const wrap = { schema_version: 1, score: 100, level: "gold" };
const jcs = canonicalizeWrapData(wrap);
// → '{"level":"gold","schema_version":1,"score":100}'
const dataHash = dataHashOf(wrap); // 32-byte Buffer
```

---

## CLI usage

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  -- \
  verify_data \
  --user <USER_ADDRESS> \
  --period <PERIOD> \
  --data <DATA_HEX>
```

`<DATA_HEX>` is the hex-encoded **canonical** UTF-8 bytes (no `0x` prefix
required by the Soroban CLI).

**Example — hash canonical JSON and verify:**

```bash
# Canonical bytes (RFC 8785). Do not pretty-print.
DATA='{"level":"gold","schema_version":1,"score":100}'
DATA_HEX=$(printf '%s' "$DATA" | xxd -p | tr -d '\n')

soroban contract invoke \
  --id C... \
  -- \
  verify_data \
  --user G... \
  --period 202401 \
  --data "$DATA_HEX"
# → true
```

---

## Matching example

Mint with canonical wrap JSON, then verify the same bytes:

```rust
// --- mint ---
let data_json = Bytes::from_slice(
    &env,
    b"{\"level\":\"gold\",\"schema_version\":1,\"score\":100}",
);
let data_hash_raw = env.crypto().sha256(&data_json);
let data_hash = BytesN::from_array(&env, &data_hash_raw.to_array());

client.mint_wrap(
    &user, &period, &archetype,
    &data_hash, &CURRENT_PAYLOAD_VERSION, &signature,
);

// --- verify with the original bytes → true ---
assert!(client.verify_data(&user, &period, &data_json));
```

---

## Non-matching examples

### Tampered field value

```rust
// Minted with {"schema_version":1,"score":100}
let original = Bytes::from_slice(&env, b"{\"schema_version\":1,\"score\":100}");
let data_hash = BytesN::from_array(&env, &env.crypto().sha256(&original).to_array());
client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &signature);

// Verifying with {"schema_version":1,"score":999} returns false — value differs
let tampered = Bytes::from_slice(&env, b"{\"schema_version\":1,\"score\":999}");
assert!(!client.verify_data(&user, &period, &tampered));
```

### Whitespace / non-canonical JSON

```rust
// Minted with RFC 8785 compact form
let compact = Bytes::from_slice(&env, b"{\"schema_version\":1,\"score\":100}");
let data_hash = BytesN::from_array(&env, &env.crypto().sha256(&compact).to_array());
client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &signature);

// Verifying with pretty-printed JSON returns false — byte sequences differ
let pretty = Bytes::from_slice(&env, b"{ \"schema_version\": 1, \"score\": 100 }");
assert!(!client.verify_data(&user, &period, &pretty));
```

### No wrap exists

```rust
// No mint has been called for this user-period combination
let data = Bytes::from_slice(&env, b"anything");
assert!(!client.verify_data(&user, &202401, &data));
// returns false, does not panic
```

---

## Batch minting (`mint_wrap_batch`)

`mint_wrap_batch` mints several `(user, period)` records under a single
aggregated signature. The aggregated signature is **not** a blanket
authorization for the batch: every record must be individually covered by the
signature, and the batch is **atomic** — if any record fails verification the
whole call reverts and no record is minted.

### Per-record authorization

Each record in the batch is verified against the aggregated signature on its
own merits. A batch that verifies as a whole is not sufficient; a signature
that covers records `A` and `B` does not authorize record `C`.

```rust
// Sign records A and B, then attempt to mint A, B, and an unsigned C.
let batch = vec![&env, record_a, record_b, record_c];
let result = client.try_mint_wrap_batch(&batch, &aggregated_signature);

// The batch fails: C is not covered by the signature.
assert!(result.is_err());
```

### Record substitution after signing

If a record is substituted after signing, the whole batch must fail rather
than minting the untampered remainder:

```rust
// Sign the original batch, then swap record_b for a tampered record_b'.
let signed = vec![&env, record_a, record_b];
let signature = sign_batch(&env, &signed);

let substituted = vec![&env, record_a, tampered_record_b];
let result = client.try_mint_wrap_batch(&substituted, &signature);

// The batch reverts; record_a is NOT minted either.
assert!(result.is_err());
assert!(client.get_wrap(&user_a, &period_a).is_none());
```

### Atomicity

Batch minting is atomic: a partial failure reverts the entire batch. There is
no "mint the valid prefix" behaviour. This matches the documented guarantee —
a batch either mints every record or mints none.

```rust
// One invalid record in the batch → no records are minted.
let result = client.try_mint_wrap_batch(&batch_with_one_bad_record, &signature);
assert!(result.is_err());

for record in batch_with_one_bad_record.iter() {
    assert!(client.get_wrap(&record.user, &record.period).is_none());
}
```

### Guards, opt-out, and per-period uniqueness

Batch minting applies the same per-record checks as single `mint_wrap`:

- **Mint guard** — a record that fails the mint guard fails the batch.
- **Opt-out** — a user who has opted out cannot be minted, even inside a batch.
- **Per-period uniqueness** — a `(user, period)` that already has a wrap cannot
  be re-minted by a batch.

```rust
// A user who opted out causes the whole batch to revert.
let result = client.try_mint_wrap_batch(&batch_including_opted_out_user, &signature);
assert!(result.is_err());

// A duplicate (user, period) causes the whole batch to revert.
let result = client.try_mint_wrap_batch(&batch_with_duplicate_period, &signature);
assert!(result.is_err());
```

---

## Off-chain workflow summary

```
Off-chain (at mint time)
─────────────────────────────────────────────────
1. Build wrap object with schema_version
2. JCS (RFC 8785) → canonical UTF-8 bytes
3. sha256(bytes)                  →  data_hash
4. Call mint_wrap(..., data_hash, ...)
5. Store the canonical bytes (or regenerable object) for the period

Off-chain (at verification time)
─────────────────────────────────────────────────
1. Recover the same object / canonical bytes
2. Call verify_data(user, period, canonical_bytes)
3. true  → on-chain hash matches; data is authentic
   false → data has been altered, or no wrap exists
```

---

## Error reference

`verify_data` itself never panics or returns an error code — it always returns
a `bool`. The only way to get a contract error is if the enclosing transaction
fails for an unrelated reason (e.g. contract not initialised before any query
that requires it). See [ERRORS.md](../ERRORS.md) for the full error catalogue.
