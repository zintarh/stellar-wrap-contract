# Canonical Signed Payload Encoding

This document defines the exact byte layout that the backend must sign and that
the `mint_wrap` entry-point verifies with `ed25519_verify`.

> **⚠ WARNING — field order is load-bearing.**
> The contract concatenates the fields shown below, in the fixed order shown,
> and passes the resulting byte string directly to `ed25519_verify`.
> Changing the order, omitting a field, or encoding any field differently will
> produce a different message digest and the signature check **will always fail**,
> causing every `mint_wrap` call to be rejected with
> `Error(Contract, #5)` (`InvalidSignature`).
>
> The single source of truth is `construct_mint_payload` in
> [`src/signature.rs`](../src/signature.rs), reproduced verbatim below. If this
> document and the code ever disagree, the code wins — please open an issue.

---

## Payload Versions

| Version | Status | Changes |
|---------|--------|---------|
| **v1** | Obsolete — rejected on-chain. `validate_payload_version` panics with `InvalidSignature` (#5). | Original layout: no expiry, no nonce. Signatures were bearer credentials with infinite lifetime. |
| **v2** | **Current** (`CURRENT_PAYLOAD_VERSION = 2`). | Adds `valid_until` (ledger timestamp, `u64`). The contract rejects `mint_wrap` / `mint_wrap_batch` when `e.ledger().timestamp() > valid_until`, surfacing `SignatureExpired` (#57). |

All downstream signers **must migrate to v2 immediately**. There is no
compatibility window — v1 payloads are rejected unconditionally via the existing
`validate_payload_version` gate *before* the signature is even verified, so
surfacing the exact error requires an on-chain v2 signature first.

---

## Algorithm (v2)

```
payload = MINT_DOMAIN_SEPARATOR            (raw bytes, NOT XDR-encoded)
        ‖ XDR(MintPayload { archetype, contract_id, data_hash,
                              payload_version, period, user, valid_until })

signature = Ed25519Sign(admin_private_key, payload)
```

`‖` denotes byte-level concatenation. There is no length prefix, separator, or
framing between fields beyond what each field's own XDR encoding already
carries.

`MINT_DOMAIN_SEPARATOR` is the constant ASCII string `"stellar-wrap-v1"`
(15 bytes). Unlike every other field, it is appended as raw bytes — it is
**not** passed through `ToXdr`. Its purpose is to bind every signature to this
specific contract/scheme so the same admin key can't be replayed against a
different Soroban contract or a future, incompatible payload format.

> **Note on the domain separator:** The separator string is *intentionally*
> unchanged from v1. Versioning is handled inside the XDR-typed
> `payload_version` field, so v1 and v2 signatures over identical parameters
> diverge *after* the separator because the typed struct payloads differ
> (v2 appends the `valid_until` integer). Reusing the separator keeps the
> off-chain signing migration surgical: only the struct layout changes, not
> the raw byte prefix.

`payload_version` is a `u32` that must equal `CURRENT_PAYLOAD_VERSION` in
[`src/mint.rs`](../src/mint.rs) (currently `2`). `mint_wrap` checks this
*before* verifying the signature and panics with `Error(Contract, #5)` if it
doesn't match. If the signing scheme ever changes, the contract will bump
`CURRENT_PAYLOAD_VERSION` and reject signatures built against the old layout,
so treat a sudden wave of `InvalidSignature` errors as a cue to check whether
the version constant moved before assuming key compromise or a client bug.

---

## Field order (v2 MintPayload struct, XDR-encoded)

The struct fields are serialized in the exact declaration order below via
Soroban's `#[contracttype]` / `ToXdr` derive. Each listed field's encoding
uses the canonical ScVal XDR representation for its Rust type.

| # | Field | Rust type | Encoding |
|---|-------|-----------|----------|
| 0 (prefix) | `MINT_DOMAIN_SEPARATOR` | `&[u8; 15]` | Raw bytes, literal ASCII `"stellar-wrap-v1"` — **not** XDR-wrapped |
| 1 (struct) | `archetype` | `Symbol` | `ToXdr` — XDR-encoded Soroban symbol (short ASCII identifier, up to 32 chars) |
| 2 (struct) | `contract_id` | `Address` (contract) | `ToXdr` on the `Env`-resolved current contract address |
| 3 (struct) | `data_hash` | `BytesN<32>` | `ToXdr` — XDR-encoded 32-byte value |
| 4 (struct) | `payload_version` | `u32` | `ToXdr` — 32-bit unsigned integer. Must equal `CURRENT_PAYLOAD_VERSION` (2 for v2). |
| 5 (struct) | `period` | `u64` | `ToXdr` — XDR-encoded unsigned 64-bit integer |
| 6 (struct) | `user` | `Address` (account) | `ToXdr` on the caller address |
| 7 (struct) | `valid_until` | `u64` | `ToXdr` — ledger timestamp (seconds since epoch) after which the signature is no longer accepted. |

### Period encoding

`period` is the canonical identifier representing the time interval of the wrap, defined as an unsigned 64-bit integer (`u64`) in `YYYYMM` format (e.g., `202401` for January 2024, `202512` for December 2025).

The semantic meaning is irrelevant to the cryptographic encoding — it is signed as a plain `u64`. Valid range: `202401`–`210012` (enforced by `validate_period` in `src/mint.rs`, returning `Error(Contract, #6)` otherwise).

`u64::MAX` is a representable, XDR-serializable value, but it is outside the valid range and is rejected with `ContractError::InvalidPeriod` before a wrap is stored. It is therefore not acceptable as a production period.

#### Non-Monthly Periods
Because the contract validation logic enforces a strict month check (`period % 100` must be between `1` and `12`) and year check (`period / 100` must be between `2024` and `2100`), non-monthly periods (e.g. daily, weekly, or quarterly periods) are not natively supported by the contract constraints.

To support non-monthly wraps, integrations and off-chain tools must map their custom period representation to a valid `YYYYMM` `u64` value before generating the signature and executing the mint transaction. For example:
- **Quarterly Wraps**: Map Q1 (Jan-Mar) to `YYYY03`, Q2 to `YYYY06`, Q3 to `YYYY09`, Q4 to `YYYY12`.
- **Weekly / Daily Wraps**: Map the week or day to the year and month of the end date of that interval.

### Archetype encoding

`archetype` is a short Soroban `Symbol` (up to 32 characters, typically
constructed with `symbol_short!(...)` for names ≤ 9 chars or `Symbol::new`
for longer ones).

### Data hash

`data_hash` is an opaque 32-byte value — commonly a SHA-256 digest of
off-chain metadata associated with the wrap. The contract does not interpret
its contents; it only stores and later returns it via `get_wrap`.

### `valid_until` (v2 addition)

`valid_until` is an unsigned 64-bit ledger timestamp, expressed in seconds
since the Unix epoch (the same unit returned by `e.ledger().timestamp()` on
Soroban). It represents the **inclusive upper bound**: the signature is
accepted when

```
ledger_timestamp <= valid_until
```

and rejected (with `ContractError::SignatureExpired`, `#57`) as soon as

```
ledger_timestamp > valid_until
```

Practically: set `valid_until = now + T` where `T` is the acceptable lifetime
of the bearer credential. A short T (minutes to hours, matching the typical
submit-to-ledger window) dramatically limits the blast radius of a leaked
signature from a backend log, queue, or support ticket. For operational
convenience during testing and one-off manual issuance, `u64::MAX` is a
legal value and produces a "never expires" signature — avoid this in
production unless you are willing to rotate `AdminPubKey` to revoke.

The check runs *after* `validate_payload_version` and `validate_period`,
but *before* the Ed25519 signature is verified, so an expired mint surfaces
a dedicated error rather than `InvalidSignature` (or worse, silently succeeds
because someone reused a leaked year-old bearer token).

---

## Reference implementation

The contract builds and verifies the payload in
[`src/signature.rs`](../src/signature.rs):

```rust
pub const MINT_DOMAIN_SEPARATOR: &[u8; 15] = b"stellar-wrap-v1";

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintPayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub period: u64,
    pub user: Address,
    pub valid_until: u64,
}

pub fn construct_mint_payload(
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

pub fn verify_mint_signature(
    e: &Env,
    admin_pubkey: &BytesN<32>,
    contract_id: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
    valid_until: u64,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let payload = construct_mint_payload(
        e, contract_id, user, period, archetype, data_hash,
        payload_version, valid_until,
    );
    // In-guest Ed25519 verification (ed25519-dalek, same 3.0.0 pin the host
    // uses) so any failure surfaces as Error(Contract, #5) instead of the
    // host's uncatchable Error(Crypto, InvalidInput) trap.
    verify_ed25519(admin_pubkey, &payload, signature)
}
```

`mint_wrap` (in `src/mint.rs`) calls `verify_mint_signature` with
`e.current_contract_address()` as `contract_id`, after checking
`payload_version == CURRENT_PAYLOAD_VERSION`, that `period` is in range,
and that the signature has not expired:

```rust
validate_period(&e, period);
validate_payload_version(&e, payload_version);

if e.ledger().timestamp() > valid_until {
    panic_with_error!(e, ContractError::SignatureExpired);
}
```

---

## Migration notes: v1 → v2

The transition is **breaking by design**: every mint caller must ship both
the new field and the bumped version together, or the contract rejects the
call with `InvalidSignature` (for wrong version) or a mismatched signature
(because the signed bytes differ). This mirrors the same version-gating
pattern that was planned for v1 from the start.

### Checklist for integrators

1. **Bump `payload_version`** from `1` → `2` everywhere your backend
   builds a signing payload (and in any hardcoded constants).
2. **Add `valid_until`** to your `MintClaim` struct and wire it through.
   Pick a short lifetime (e.g. 15 minutes = `now + 15 * 60`) unless you
   have a concrete reason to allow longer-lived bearer signatures.
   Pass `u64::MAX` only during the transition cut-over window and for
   internal tooling.
3. **Include `valid_until` in the signed struct** at position 7 (after
   `user`). The contract serializes `MintPayload` by declaration order,
   so inserting it anywhere else is equivalent to signing a different
   message and will fail `InvalidSignature`.
4. **Pass `valid_until` as a new argument** to `mint_wrap` (7th argument,
   inserted between `payload_version` and `signature`) and to each
   `BatchWrapItem` in `mint_wrap_batch`.
5. **Handle `SignatureExpired` (#57)** client-side. In the old scheme, a
   stale bearer token would either succeed (bad — replay of a leaked sig)
   or surface `InvalidSignature` depending on what else changed. In v2, a
   signature whose deadline has passed returns the dedicated error code
   `Error(Contract, #57)`, which your UI / CLI should map to a human
   message suggesting the user request a fresh signature from the backend.
6. **Drop v1 support entirely.** There is no hybrid mode — once the
   on-chain `CURRENT_PAYLOAD_VERSION` is `2`, any v1 payload hits
   `validate_payload_version` before crypto is even run. No server-side
   dual-sign or dual-accept rollout is required; just deploy a backend
   that emits v2 and upgrade your clients in lock-step.

### Why there is no nonce (and what expiry buys instead)

The original issue discussed both nonces and timestamps. We intentionally
ship **only** `valid_until` for v2, and not a per-signature nonce field,
because:

- `(user, period)` is already a globally unique composite key on this
  contract — a successful mint writes `Wrap(user, period)` and any replay
  of the same pair hits `WrapAlreadyExists` before the signature is even
  re-verified. The infinite-lifetime problem is the real risk, not the
  "same-user-same-period twice" risk (which is already covered by the
  unique key).
- Adding a nonce would require on-chain storage to mark nonces as spent
  (or a K-ordered window with pruning), and none of the on-chain primitives
  in this contract currently have the shape to support that cleanly.
  `valid_until` bounds the attack window at zero incremental storage cost,
  matching the threat model (leaked creds sitting in logs indefinitely)
  far better than an unbounded nonce set would.

If a future release needs finer-grained revocation (e.g. per-claim
blacklisting independent of `AdminPubKey` rotation), we will bump the
payload version again — the versioning fence is in place precisely to make
that follow-up safe.

---

## Key management for backend integrators

The admin private key signs every mint claim on this contract, so treat it
like any other high-value signing key:

- **Never** embed, log, or transmit the admin private key to a frontend,
  mobile client, or any component outside a trusted backend process. Only the
  signing service should ever hold it in memory.
- Store it in a secrets manager, KMS, or HSM rather than in source control,
  plain environment files, or container images. Prefer signing via a KMS/HSM
  API (which returns only the signature) over pulling the raw key material
  into application memory when your infrastructure supports it.
- `AdminPubKey` rotation is exposed on-chain via `update_admin_pubkey` and
  can also be gated through the timelock controller (`TimelockAction::SetAdminPubKey`).
  Rotation invalidates every outstanding signature atomically — keep a rollover
  playbook handy and pair it with short `valid_until` lifetimes so you rarely
  need to pull the emergency brake.
- Rate-limit and audit-log every signature your backend issues. Since a
  signature is a bearer credential for one specific `(user, period,
  archetype, data_hash, valid_until)` claim, logging *what* was signed (not the key
  itself) gives you an audit trail independent of the chain.

---

## Backend signing example (TypeScript)

This mirrors `construct_mint_payload` field-for-field using
[`@stellar/stellar-sdk`](https://www.npmjs.com/package/@stellar/stellar-sdk),
which exposes the same XDR/`ScVal` encoding the Soroban host uses on the Rust
side. Treat this as a reference for the byte layout, not a copy-paste
production signer — wire the admin secret through your own KMS/HSM integration
instead of `Keypair.fromSecret`.

```ts
import { Address, Keypair, nativeToScVal, xdr } from "@stellar/stellar-sdk";

// Literal ASCII bytes — must match MINT_DOMAIN_SEPARATOR in src/signature.rs
// exactly. This is NOT XDR-encoded, unlike every field below it.
const MINT_DOMAIN_SEPARATOR = Buffer.from("stellar-wrap-v1", "ascii");

// Must equal CURRENT_PAYLOAD_VERSION in src/mint.rs. Bump both together.
const CURRENT_PAYLOAD_VERSION = 2;

interface MintClaim {
  contractId: string; // "C..." Soroban contract address
  user: string; // "G..." Stellar account address
  period: bigint; // YYYYMM as u64, e.g. 202508n
  archetype: string; // Soroban Symbol, <= 32 chars
  dataHash: Buffer; // 32 bytes
  validUntil: bigint; // Ledger timestamp (seconds since epoch); use 0n for "never"
}

/** Builds the exact byte string the contract passes to ed25519_verify.
 *
 *  The typed MintPayload struct is XDR-encoded as a whole (matching
 *  soroban-sdk's `#[contracttype]` derive), then concatenated after
 *  the raw domain separator. The struct field order inside the XDR is:
 *    archetype, contract_id, data_hash, payload_version, period, user, valid_until
 *  and must not be rearranged.
 */
function buildMintPayload(claim: MintClaim): Buffer {
  const archetypeSc = nativeToScVal(claim.archetype, { type: "symbol" });
  const contractSc = Address.fromString(claim.contractId).toScVal();
  const hashSc = nativeToScVal(claim.dataHash, { type: "bytes" });
  const versionSc = nativeToScVal(CURRENT_PAYLOAD_VERSION, { type: "u32" });
  const periodSc = nativeToScVal(claim.period, { type: "u64" });
  const userSc = Address.fromString(claim.user).toScVal();
  const untilSc = nativeToScVal(claim.validUntil, { type: "u64" });

  const structFields = xdr.ScVal.scvMap([
    // IMPORTANT: map entries must be sorted by key-name ASCII order.
    // The keys below match the exact identifiers of the Rust MintPayload
    // struct; reordering or renaming them breaks signature verification.
    new xdr.ScMapEntry({ key: nativeToScVal("archetype", { type: "symbol" }), val: archetypeSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("contractId", { type: "symbol" }), val: contractSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("dataHash", { type: "symbol" }), val: hashSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("payloadVersion", { type: "symbol" }), val: versionSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("period", { type: "symbol" }), val: periodSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("user", { type: "symbol" }), val: userSc }),
    new xdr.ScMapEntry({ key: nativeToScVal("validUntil", { type: "symbol" }), val: untilSc }),
  ]);

  return Buffer.concat([
    MINT_DOMAIN_SEPARATOR,
    structFields.toXDR(),
  ]);
}

/**
 * Signs a mint claim with the admin key. `adminKeypair` should come from your
 * KMS/HSM/secrets-manager integration, never from a hard-coded secret.
 *
 * In production, set `validUntil = now() + FIFTEEN_MINUTES` (or shorter) and
 * regenerate a fresh signature if a submission lands after the deadline.
 */
function signMintClaim(adminKeypair: Keypair, claim: MintClaim): Buffer {
  const payload = buildMintPayload(claim);
  return adminKeypair.sign(payload); // raw 64-byte Ed25519 signature
}
```

Call `mint_wrap(user, period, archetype, data_hash, payload_version,
valid_until, signature)` on the contract with the same `period`, `archetype`,
`data_hash`, `payload_version`, and `valid_until` used to build the payload
above, plus the resulting 64-byte signature. Any mismatch between what was
signed and what is submitted produces `Error(Contract, #5)`; a correct
signature but expired deadline produces `Error(Contract, #57)`.

---

## Test vectors

The test suite in `src/signature.rs` (`#[cfg(test)] mod tests`) provides
exercisable vectors, e.g. `test_verify_mint_signature_accepts_valid_signature`
and `test_verify_mint_signature_rejects_wrong_key`. Expiry edge cases
(just-before, at, just-after the deadline) live in `src/expiration_test.rs`
as `test_signature_expiry_*`.

Reproduce them in Rust with the helper:

```rust
use ed25519_dalek::{Signer, SigningKey};

fn sign_payload(
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
        env, contract, user, period, archetype, data_hash,
        payload_version, valid_until,
    );

    let len = payload.len() as usize;
    let mut out = vec![0u8; len];
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}
```

Any byte modification to the payload (including reordering fields, omitting
`valid_until`, or signing with the wrong `payload_version`) produces a
different message, and the contract's in-guest Ed25519 verification
(`verify_ed25519` in `src/signature.rs`) will panic with `Error(Contract, #5)`.

---

## Error reference

| Code | Name | Triggered when |
|------|------|----------------|
| `#3` | `Unauthorized` | `user.require_auth()` fails |
| `#5` | `InvalidSignature` | `payload_version` doesn't match `CURRENT_PAYLOAD_VERSION`, or in-guest Ed25519 verification rejects the signature (wrong payload order/fields, wrong key, corrupted bytes). Also raised on v1 payloads because the version gate fires first. |
| `#6` | `InvalidPeriod` | `period` is outside `202401`–`210012` |
| `#57` | `SignatureExpired` | v2 payloads only. The current ledger timestamp is strictly greater than `valid_until`; request a fresh signature from the backend with a later deadline. |

See [ERRORS.md](../ERRORS.md) for the full error catalogue.
