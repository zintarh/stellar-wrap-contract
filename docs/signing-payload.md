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

## Algorithm

```
payload = MINT_DOMAIN_SEPARATOR            (raw bytes, NOT XDR-encoded)
        ‖ XDR(payload_version)
        ‖ XDR(contract_id)
        ‖ XDR(user_address)
        ‖ XDR(period)
        ‖ XDR(valid_until)
        ‖ XDR(archetype)
        ‖ XDR(data_hash)

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

`payload_version` is a `u32` that must equal `CURRENT_PAYLOAD_VERSION` in
[`src/mint.rs`](../src/mint.rs) (currently `2`). `mint_wrap` checks this
*before* verifying the signature and panics with `Error(Contract, #5)` if it
doesn't match. If the signing scheme ever changes, the contract will bump
`CURRENT_PAYLOAD_VERSION` and reject signatures built against the old layout,
so treat a sudden wave of `InvalidSignature` errors as a cue to check whether
the version constant moved before assuming key compromise or a client bug.

The signed payload also includes `valid_until`, a ledger timestamp deadline.
The contract rejects any mint whose current ledger timestamp is greater than
`valid_until` with `Error(Contract, #53)` (`SignatureExpired`). This keeps an
otherwise valid signature from being reused forever if it goes unused.

---

## Field order

| # | Field | Rust type | Encoding |
|---|-------|-----------|----------|
| 1 | `MINT_DOMAIN_SEPARATOR` | `&[u8; 15]` | Raw bytes, literal ASCII `"stellar-wrap-v1"` — **not** XDR-wrapped |
| 2 | `payload_version` | `u32` | `ToXdr` — same XDR integer treatment as `period` below, just 32 bits wide |
| 3 | `contract_id` | `Address` (contract) | `ToXdr` on the `Env`-resolved current contract address |
| 4 | `user_address` | `Address` (account) | `ToXdr` on the caller address |
| 5 | `period` | `u64` | `ToXdr` — XDR-encoded unsigned 64-bit integer |
| 6 | `valid_until` | `u64` | `ToXdr` — ledger timestamp deadline for this signature |
| 7 | `archetype` | `Symbol` | `ToXdr` — XDR-encoded Soroban symbol (short ASCII identifier, up to 32 chars) |
| 8 | `data_hash` | `BytesN<32>` | `ToXdr` — XDR-encoded 32-byte value |

### Period encoding

`period` is an integer in `YYYYMM` format (e.g. `202401` for January 2024).
The semantic meaning is irrelevant to the encoding — it is signed as a plain
`u64`. Valid range: `202401`–`210012` (enforced by `validate_period` in
`src/mint.rs`, `Error(Contract, #6)` otherwise).

### Archetype encoding

`archetype` is a short Soroban `Symbol` (up to 32 characters, typically
constructed with `symbol_short!(...)` for names ≤ 9 chars or `Symbol::new`
for longer ones).

### Data hash

`data_hash` is an opaque 32-byte value — commonly a SHA-256 digest of
off-chain metadata associated with the wrap. The contract does not interpret
its contents; it only stores and later returns it via `get_wrap`.

---

## Reference implementation

The contract builds and verifies the payload in
[`src/signature.rs`](../src/signature.rs):

```rust
pub const MINT_DOMAIN_SEPARATOR: &[u8; 15] = b"stellar-wrap-v1";

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
    payload.append(&payload_version.to_xdr(e));
    payload.append(&contract_id.to_xdr(e));
    payload.append(&user.clone().to_xdr(e));
    payload.append(&period.to_xdr(e));
    payload.append(&valid_until.to_xdr(e));
    payload.append(&archetype.clone().to_xdr(e));
    payload.append(&data_hash.clone().to_xdr(e));
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
        e,
        contract_id,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
        valid_until,
    );
    // In-guest Ed25519 verification (ed25519-dalek, same 3.0.0 pin the host
    // uses) so any failure surfaces as Error(Contract, #5) instead of the
    // host's uncatchable Error(Crypto, InvalidInput) trap.
    verify_ed25519(admin_pubkey, &payload, signature)
}
```

`mint_wrap` (in `src/mint.rs`) calls `verify_mint_signature` with
`e.current_contract_address()` as `contract_id`, after checking
`payload_version == CURRENT_PAYLOAD_VERSION` and that `period` is in range.

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
- **There is currently no on-chain rotation entrypoint for the signing key.**
  `AdminPubKey` is set exactly once, in `initialize`, and nothing in
  `src/admin.rs` or `src/lib.rs` updates it afterwards — this is distinct
  from `update_admin` / `propose_admin` / `accept_admin`, which only rotate
  the *admin address* used for authorization, not the Ed25519 signing key
  used for mint claims. If you suspect the signing key is compromised, you
  need a contract upgrade/migration to introduce rotation; plan your key
  custody (HSM, strict access control, offline backup of the public key)
  accordingly.
- Rate-limit and audit-log every signature your backend issues. Since a
  signature is a bearer credential for one specific `(user, period,
  archetype, data_hash)` claim, logging *what* was signed (not the key
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
import { Address, Keypair, nativeToScVal } from "@stellar/stellar-sdk";

// Literal ASCII bytes — must match MINT_DOMAIN_SEPARATOR in src/signature.rs
// exactly. This is NOT XDR-encoded, unlike every field below it.
const MINT_DOMAIN_SEPARATOR = Buffer.from("stellar-wrap-v1", "ascii");

// Must equal CURRENT_PAYLOAD_VERSION in src/mint.rs. Bump both together.
const CURRENT_PAYLOAD_VERSION = 2;

interface MintClaim {
  contractId: string; // "C..." Soroban contract address
  user: string; // "G..." Stellar account address
  period: number; // YYYYMM, e.g. 202508
  validUntil: bigint; // ledger timestamp deadline
  archetype: string; // Soroban Symbol, <= 32 chars
  dataHash: Buffer; // 32 bytes
}

/** Builds the exact byte string the contract passes to ed25519_verify. */
function buildMintPayload(claim: MintClaim): Buffer {
  return Buffer.concat([
    MINT_DOMAIN_SEPARATOR,
    nativeToScVal(CURRENT_PAYLOAD_VERSION, { type: "u32" }).toXDR(),
    Address.fromString(claim.contractId).toScVal().toXDR(),
    Address.fromString(claim.user).toScVal().toXDR(),
    nativeToScVal(BigInt(claim.period), { type: "u64" }).toXDR(),
    nativeToScVal(claim.validUntil, { type: "u64" }).toXDR(),
    nativeToScVal(claim.archetype, { type: "symbol" }).toXDR(),
    nativeToScVal(claim.dataHash, { type: "bytes" }).toXDR(),
  ]);
}

/**
 * Signs a mint claim with the admin key. `adminKeypair` should come from your
 * KMS/HSM/secrets-manager integration, never from a hard-coded secret.
 */
function signMintClaim(adminKeypair: Keypair, claim: MintClaim): Buffer {
  const payload = buildMintPayload(claim);
  return adminKeypair.sign(payload); // raw 64-byte Ed25519 signature
}
```

Call `mint_wrap(user, period, archetype, data_hash, payload_version,
valid_until, signature)` on the contract with the same `period`, `valid_until`,
`archetype`, `data_hash`, and `payload_version` used to build the payload above,
plus the resulting
64-byte signature. Any mismatch between what was signed and what is submitted
produces `Error(Contract, #5)`.

---

## Test vectors

The test suite in `src/signature.rs` (`#[cfg(test)] mod tests`) provides
exercisable vectors, e.g. `test_verify_mint_signature_accepts_valid_signature`
and `test_verify_mint_signature_rejects_wrong_key`. Reproduce them in Rust
with:

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
) -> BytesN<64> {
    let payload = construct_mint_payload(env, contract, user, period, archetype, data_hash, payload_version);

    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);

    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}
```

Any byte modification to the payload (including reordering fields, or signing
with the wrong `payload_version`) produces a different message, and the
contract's in-guest Ed25519 verification (`verify_ed25519` in `src/signature.rs`)
will panic with `Error(Contract, #5)`.

---


## Error reference

| Code | Name | Triggered when |
|------|------|----------------|
| `#3` | `Unauthorized` | `user.require_auth()` fails |
| `#5` | `InvalidSignature` | `payload_version` doesn't match `CURRENT_PAYLOAD_VERSION`, or in-guest Ed25519 verification rejects the signature (wrong payload order/fields, wrong key, corrupted bytes) |
| `#6` | `InvalidPeriod` | `period` is outside `202401`–`210012` |

See [ERRORS.md](../ERRORS.md) for the full error catalogue.
