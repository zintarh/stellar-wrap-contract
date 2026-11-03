# Mint flow for integrators

End-to-end path for a successful `mint_wrap`: wallet → frontend → signer →
Soroban RPC → contract → indexer. Use this as the onboarding map; deep details
live in the linked docs.

Related:

- [Canonical signed payload encoding](signing-payload.md) — what the signer
  must Ed25519-sign
- [Using `verify_data`](verify-data.md) — how to check that off-chain JSON still
  matches the on-chain `data_hash`
- [ERRORS.md](../ERRORS.md) — full contract error catalogue

---

## Sequence diagram

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant Wallet as Wallet<br/>(e.g. Freighter)
    participant FE as Frontend
    participant Signer as Signer backend<br/>(admin Ed25519 key)
    participant RPC as Soroban RPC
    participant Contract as Wrap contract
    participant Indexer as Indexer

    User->>FE: Request mint (period, archetype, wrap JSON)
    FE->>FE: Canonicalize JSON → SHA-256 → data_hash
    FE->>Signer: Request signature<br/>(contract_id, user, period,<br/>archetype, data_hash, payload_version)
    Signer->>Signer: Build canonical mint payload<br/>(see signing-payload.md)
    Signer-->>FE: 64-byte Ed25519 signature

    FE->>Wallet: Build mint_wrap tx; request auth
    Wallet-->>User: Approve transaction
    User-->>Wallet: Confirm
    Wallet-->>FE: Signed auth / transaction

    FE->>RPC: Submit mint_wrap(...)
    RPC->>Contract: Invoke mint_wrap
    Contract->>Contract: Pause / auth / period /<br/>payload_version / signature /<br/>opt-out / uniqueness checks
    Contract->>Contract: Store WrapRecord; emit mint event
    Contract-->>RPC: Success
    RPC-->>FE: Transaction result
    FE-->>User: Mint confirmed

    Indexer->>RPC: Poll / stream events
    RPC-->>Indexer: mint event (user, period, archetype)
    Indexer->>Indexer: Index wrap for queries / leaderboards
```

Batch minting (`mint_wrap_batch`) follows the same off-chain prep per item, then
one RPC call with an aggregated or per-item signature. See
[verify-data.md — Batch minting](verify-data.md#batch-minting-mint_wrap_batch)
and the README batch section for atomicity and signature modes.

---

## Computing `data_hash` from wrap JSON

The contract never sees the wrap JSON. It only stores a 32-byte commitment:

```
raw_bytes  = UTF-8 bytes of the wrap JSON (exact sequence you will keep)
data_hash  = SHA-256(raw_bytes)          → BytesN<32> passed to mint_wrap
```

**Canonical JSON is strongly recommended** so every party can reproduce the same
bytes. Whitespace, key order, trailing newlines, and BOMs all change the digest.
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785) (JSON Canonicalization
Scheme) is one well-specified option.

At mint time:

1. Produce canonical (or otherwise fixed) JSON bytes.
2. `data_hash = sha256(bytes)`.
3. Ask the signer to sign a payload that includes that `data_hash`.
4. Call `mint_wrap(..., data_hash, payload_version, signature)`.
5. Persist the original bytes off-chain for later checks.

After mint, call `verify_data(user, period, raw_bytes)` to confirm the stored
hash still matches. Full preparation rules, pitfalls, and examples:
[docs/verify-data.md](verify-data.md).

---

## Step-by-step and client-visible errors

Errors below are what a client typically sees at each stage. Contract panics
surface as `Error(Contract, #N)` in the Soroban transaction result. Auth
failures from `require_auth` may appear as host auth errors rather than a
`ContractError` code, depending on the SDK/RPC wrapper.

### 1. Frontend — prepare claim

| Failure | What the client sees |
|---------|----------------------|
| Invalid wrap JSON / failed canonicalisation | App/validation error (off-chain; no contract call) |
| Wrong period shape before signing | Prefer catching locally; contract later returns `#6` `InvalidPeriod` |

### 2. Signer backend — issue signature

| Failure | What the client sees |
|---------|----------------------|
| Signer rejects claim (policy, rate limit, unknown user) | HTTP / API error from your signer |
| Signer unavailable or misconfigured key | HTTP 5xx / signing failure |
| Payload built with wrong field order or encoding | Later `#5` `InvalidSignature` on-chain (see [signing-payload.md](signing-payload.md)) |

The admin private key must never leave the signer. See key-management notes in
[signing-payload.md](signing-payload.md#key-management-for-backend-integrators).

### 3. Wallet — authorize `mint_wrap`

| Failure | What the client sees |
|---------|----------------------|
| User rejects in wallet | Wallet rejection / cancelled signing |
| Wrong network or unfunded account | Wallet / RPC fee or network error |
| `user` in the call is not the authorizing address | Auth failure / `#3` `Unauthorized` (depending on host surface) |

### 4. Soroban RPC — simulate / submit

| Failure | What the client sees |
|---------|----------------------|
| RPC down, timeout, or bad endpoint | Transport / HTTP error |
| Simulation fails | Simulation error embedding the contract/host error below |
| Insufficient fee / tx rejected by network | RPC submit error |

### 5. Contract — `mint_wrap` checks (in order)

| Code | Name | When |
|-----:|------|------|
| `#12` | `Paused` | Contract is paused (`require_not_paused`) |
| — / `#3` | Auth / `Unauthorized` | `user.require_auth()` fails |
| `#6` | `InvalidPeriod` | `period` not valid `YYYYMM` (year 2024–2100, month 01–12) |
| `#5` | `InvalidSignature` | `payload_version ≠ CURRENT_PAYLOAD_VERSION` (currently `1`), or Ed25519 verify fails |
| `#2` | `NotInitialized` | Admin pubkey missing (contract never initialized) |
| `#32` | `UserOptedOut` | User has opted out of wraps |
| `#4` | `WrapAlreadyExists` | A wrap already exists for `(user, period)` |
| `#13` | `ArithmeticOverflow` | Wrap or storage-byte counter would overflow (rare) |
| `#51` | `StorageInvariantViolation` | Legacy user missing `WrapPeriods` index (needs `backfill_wrap_periods`) |

Signature layout and troubleshooting for `#5`:
[signing-payload.md](signing-payload.md).

### 6. Indexer — consume `mint` event

Successful mints emit a `mint` event (`user`, `period`, `archetype`) only after
storage writes succeed. Indexer-side failures (lag, missed cursor, decode bugs)
are off-chain; they do not roll back the mint. Re-scan from the successful
transaction if needed. Event shape: README “Event schema”.

### 7. Optional post-mint integrity check

`verify_data(user, period, data)` returns a `bool` and does **not** panic with
a mint error code:

| Result | Meaning |
|--------|---------|
| `true` | `sha256(data)` equals stored `data_hash` |
| `false` | Bytes differ, or no wrap for `(user, period)` |

Details: [verify-data.md](verify-data.md).

---

## Minimal happy path checklist

1. Canonical JSON → `data_hash = SHA-256(bytes)`.
2. Signer returns Ed25519 signature over the payload in
   [signing-payload.md](signing-payload.md).
3. Wallet authorizes `mint_wrap(user, period, archetype, data_hash, 1, signature)`.
4. RPC reports success; indexer sees one `mint` event.
5. (Optional) `verify_data` returns `true` for the same bytes.
