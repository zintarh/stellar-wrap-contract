# Security Recommendations for Stellar Wrap Contract

## Overview
This document describes the security properties of the deployed contract, identifies
remaining pre-mainnet work, and provides guidance for auditors.

For developer-facing error handling (e.g. `Error(Contract, #4)`), see the
[Error Reference (ERRORS.md)](./ERRORS.md).

---

## ✅ Signature Verification — Implemented (Ed25519)

### Current State
`mint_wrap()` and `update_wrap()` perform real Ed25519 cryptographic signature
verification using Soroban's built-in `e.crypto().ed25519_verify()`. The old
unconditional stub (`fn verify_signature() -> bool { true }`) no longer exists.

### How It Works

The backend generates a **canonical payload** and signs it with the admin Ed25519
private key. The contract reconstructs the same payload and verifies the signature
against the stored admin public key before minting.

#### Payload construction (`mint_wrap`)

```
payload = XDR(contract_address)
        ‖ XDR(user)
        ‖ XDR(period)        // u64 — prevents period replay
        ‖ XDR(archetype)
        ‖ XDR(data_hash)     // SHA-256 of off-chain JSON
```

Each field is XDR-encoded before concatenation, which provides unambiguous
length-delimited framing and prevents field-boundary collisions.

#### On-chain verification (Rust)

```rust
let mut payload = Bytes::new(&e);
payload.append(&e.current_contract_address().to_xdr(&e));   // cross-contract replay protection
payload.append(&user.clone().to_xdr(&e));                   // identity binding
payload.append(&period.to_xdr(&e));                         // period binding
payload.append(&archetype.clone().to_xdr(&e));
payload.append(&data_hash.clone().to_xdr(&e));

// Panics with ContractError::InvalidSignature (code 6) on failure
e.crypto().ed25519_verify(&admin_pubkey, &payload, &signature);
```

`ed25519_verify` panics (traps) when verification fails — the transaction is
rolled back and no state is written.

#### Off-chain signing (TypeScript example)

```typescript
import { xdr, Address } from "@stellar/stellar-sdk";
import * as nacl from "tweetnacl";

function buildMintPayload(
  contractId: string,
  userAddress: string,
  period: bigint,
  archetype: string,
  dataHash: Uint8Array
): Uint8Array {
  const parts: Uint8Array[] = [
    xdr.ScAddress.scAddressTypeContract(Buffer.from(contractId, "hex")).toXDR(),
    Address.fromString(userAddress).toXDR(),
    xdr.Uint64.fromBigInt(period).toXDR(),
    xdr.ScSymbol.encode(archetype),
    Buffer.from(dataHash),
  ];
  return Buffer.concat(parts);
}

const payload  = buildMintPayload(contractId, user, period, archetype, dataHash);
const signature = nacl.sign.detached(payload, adminSecretKey);
// Pass signature (64 bytes) as the `signature` argument to mint_wrap()
```

### Security Properties Provided

| Property | How it is enforced |
|---|---|
| **Identity binding** | `user` is in the payload; a signature for Alice cannot be used by Bob |
| **Cross-contract replay** | `contract_address` is the first payload field |
| **Period replay** | `period` is in the payload; same user cannot reuse a signature for a different period |
| **Data integrity** | `data_hash` (SHA-256 of JSON) is in the payload and also stored on-chain |
| **Duplicate prevention** | `WrapAlreadyExists` check after signature verification |

---

## ✅ Security Features Already Implemented

### 1. Replay Attack Protection
**Status:** IMPLEMENTED

```rust
let wrap_key = DataKey::Wrap(user.clone(), period);
if e.storage().persistent().has(&wrap_key) {
    panic_with_error!(e, ContractError::WrapAlreadyExists);
}
```

The same `(user, period)` pair can never be minted twice regardless of whether the
caller has a valid signature.

**Test coverage:**
- `test_replay_attack_same_period_fails` ✓
- `test_duplicate_period_fails` ✓

### 2. Authorization Protection
**Status:** IMPLEMENTED

```rust
user.require_auth();       // user must sign the Stellar transaction
// … then …
e.crypto().ed25519_verify(&admin_pubkey, &payload, &signature);  // admin must pre-sign payload
```

Both conditions must hold. An attacker who controls the Stellar keypair of a user
still cannot forge the Ed25519 admin signature, and vice-versa.

**Test coverage:**
- `test_mint_wrap_unauthorized` ✓
- `test_non_admin_cannot_mint` ✓

### 3. Initialization Protection
**Status:** IMPLEMENTED

```rust
if e.storage().instance().has(&DataKey::Admin) {
    panic_with_error!(e, ContractError::AlreadyInitialized);
}
```

**Test coverage:**
- `test_initialize_twice_fails` ✓

### 4. Reentrancy Guard
**Status:** IMPLEMENTED

A temporary-storage guard (`DataKey::MintGuard`) is set at the start of `mint_wrap`
and removed on success. If execution panics mid-flight, Soroban's ledger rollback
prevents the guard from persisting incorrectly.

### 5. Zero-Hash Rejection
**Status:** IMPLEMENTED

```rust
if data_hash == BytesN::from_array(&e, &ZERO_HASH_BYTES) {
    panic_with_error!(e, ContractError::InvalidDataHash);
}
```

All-zero `data_hash` values are rejected to guard against missing or
uninitialized data.

### 6. Timestamp Integrity
**Status:** SECURE

The contract uses `e.ledger().timestamp()` rather than accepting a
user-supplied timestamp. Ledger timestamps are set by consensus and cannot be
forged by a transaction submitter.

**Test coverage:**
- `test_timestamp_is_from_ledger_not_user` ✓

### 7. Archetype Allowlist
**Status:** IMPLEMENTED

Archetypes are validated against an admin-managed allowlist stored in instance
storage. Unknown archetypes are rejected with `InvalidArchetype`.

```rust
Self::validate_archetype(&e, &archetype);
```

### 8. Cross-Contract Replay Protection
**Status:** IMPLEMENTED

`e.current_contract_address()` is the first field in every signed payload. A
signature issued for Contract V1 will fail verification on Contract V2 (different
address) even if all other fields are identical.

**Test coverage:**
- `test_cross_contract_replay_protection` ✓

---

## ⚠️ Remaining Pre-Mainnet Items

### 1. Signature Expiry / `expiry_ledger`
**Status:** PARTIALLY IMPLEMENTED

`mint_wrap` accepts an `expiry_ledger: u32` parameter and `sign_payload` includes
it in the payload in tests. The on-chain check that rejects signatures past their
expiry ledger should be confirmed present and tested end-to-end before mainnet.

**Recommendation:** Verify that `mint_wrap` explicitly compares
`e.ledger().sequence() > expiry_ledger` and panics with an appropriate error.
Add a test that a signature with a past `expiry_ledger` is rejected.

### 2. Admin Key Management
**Status:** OPERATIONAL CONCERN

The `admin_pubkey` is stored in instance storage and verified on every mint. The
corresponding private key must be held securely (HSM or similar). Key rotation
requires `update_admin()` followed by redeployment of a new signing service.

**Recommendation:** Document the key rotation procedure and rehearse it before
mainnet.

### 3. Third-Party Security Audit
**Status:** PENDING

**Recommendation:** Engage an independent Soroban auditor before handling real
user data at scale.

### 4. Fuzz Testing
**Status:** PENDING

Consider adding property-based or fuzz tests:
- No user should have duplicate periods.
- Sum of all `WrapCount` values should equal `TotalMints`.
- Any random 64-byte blob passed as `signature` must be rejected.

```bash
cargo install cargo-fuzz
cargo fuzz init
cargo fuzz run fuzz_target_1
```

### 5. Upgrade Key Control
**Status:** OPERATIONAL CONCERN

`upgrade()` requires admin authorization. If the admin keypair is compromised, an
attacker could replace the WASM. Consider a time-lock or multi-sig admin for
production.

---

## 📊 Gas / Resource Analysis

```bash
# Run gas-annotated tests
cargo test test_gas_analysis -- --nocapture
```

### Known cost breakdown

| Operation | Notable costs |
|---|---|
| `mint_wrap` | 2 persistent writes (`WrapRecord` + `WrapCount`), 1 temp write/remove (guard), 1 event |
| `has_wrap` | 1 persistent `has()` call — no deserialization, cheaper than `get_wrap` |
| `get_wrap` | 1 persistent read + XDR deserialization of `WrapRecord` |
| `balance_of` | 1 persistent read (`WrapCount`) |

---

## 🧪 Test Suite Summary

### Security Tests (`src/security_test.rs`)

| Test | Purpose | Expected Behavior |
|------|---------|-------------------|
| `test_replay_attack_same_period_fails` | Replay protection | PANIC #4 |
| `test_replay_attack_different_hash_same_period_fails` | Duplicate period prevention | PANIC #4 |
| `test_multiple_periods_for_same_user_success` | Valid multi-period usage | SUCCESS |
| `test_signature_cannot_be_stolen_by_another_user` | Identity theft prevention | SUCCESS (isolation) |
| `test_cross_contract_replay_protection` | Cross-contract isolation | SUCCESS (independent storage) |
| `test_gas_analysis_mint_operation` | Resource consumption | Prints metrics |
| `test_gas_analysis_multiple_mints` | Scaling analysis | Prints metrics |
| `test_timestamp_is_from_ledger_not_user` | Timestamp integrity | SUCCESS |
| `test_edge_case_long_symbols` | Symbol length limits | SUCCESS |
| `test_non_admin_cannot_mint` | Authorization check | PANIC |

### Running Tests

```bash
# Run all tests
cargo test

# Run only security tests
cargo test security_test

# Run with output for gas analysis
cargo test test_gas_analysis -- --nocapture

# Run with detailed output
cargo test -- --nocapture --test-threads=1
```

---

## 🚀 Pre-Mainnet Checklist

- [x] Ed25519 signature verification implemented (`e.crypto().ed25519_verify`)
- [x] Replay attack protection implemented (`WrapAlreadyExists` check)
- [x] Admin authorization implemented (`require_auth`)
- [x] Duplicate period prevention implemented
- [x] Timestamp integrity — uses ledger timestamp, not user-supplied
- [x] Reentrancy guard (temporary storage `MintGuard`)
- [x] Zero data_hash rejection (`InvalidDataHash`)
- [x] Cross-contract replay protection (contract address in payload)
- [x] Archetype allowlist validation
- [x] Comprehensive unit + security test suite
- [ ] Confirm `expiry_ledger` check is enforced on-chain and tested
- [ ] Rehearse admin key rotation procedure
- [ ] Run gas analysis and document costs for each entry point
- [ ] Third-party security audit
- [ ] Fuzz testing with property-based tests
- [ ] Load/stress testing for high-volume scenarios

---

## 📚 Additional Security Best Practices

### Invariant Testing

Consider property-based tests verifying:
- No user ever has duplicate periods.
- Total wraps minted equals sum of all user `WrapCount` values.
- Timestamps are monotonic within a session.

### Access Control Review

- Confirm `initialize()` is called exactly once during deployment.
- Verify admin key is stored securely (HSM or equivalent) in production.
- Consider multi-sig admin (`require_auth_for_args`) for production upgrade control.

---

## 🔗 References

- [Soroban Security Best Practices](https://soroban.stellar.org/docs/learn/security)
- [Stellar Smart Contract Audit Guidelines](https://stellar.org/developers)
- [Soroban Auth Framework](https://soroban.stellar.org/docs/learn/authorization)
- [Soroban `ed25519_verify` docs](https://docs.rs/soroban-sdk/latest/soroban_sdk/crypto/struct.Crypto.html)
