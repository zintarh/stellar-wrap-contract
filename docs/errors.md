# Error Reference (ContractError) — Stellar Wrap

This document maps the on-chain contract error codes surfaced by Soroban to their meaning and fixes.

Soroban surfaces contract panics as an `Error(Contract, #N)` string in transaction results.

The codes are defined by the Rust `ContractError` enum in `src/lib.rs`.

---

## Quick table: error code → meaning

| Code | Variant name | Human-readable description | Common cause | Resolution steps |
|---:|---|---|---|---|
| 1 | `AlreadyInitialized` | `initialize()` was called after the contract was already initialized | Deployment scripts/tests calling `initialize` twice | 1) Ensure you call `initialize()` exactly once. 2) If using an upgrade flow, remember upgrades do **not** require calling `initialize()` again. |
| 2 | `NotInitialized` | A function requiring initialization was called before `initialize()` ran | Missing/incorrect deployment step; wrong contract instance/address | 1) Verify you are calling the correct contract instance. 2) Call `initialize(admin, admin_pubkey)` once. 3) Re-check your client/deployer wiring. |
| 3 | `Unauthorized` | Caller is not allowed (admin-only function called by non-admin, or reentrancy guard tripped) | - Calling an admin-only function from a non-admin address, or missing `require_auth()`
- Reentrancy guard indicates an unexpected execution pattern / guard collision | 1) For admin-only functions (`update_wrap`, `revoke_wrap`, `upgrade`), ensure the call includes admin authorization. 2) For `mint_wrap`, ensure the `user` parameter is the address authorizing the call.
3) If you are seeing this during retries, check for concurrent calls or repeated invocations that might trip the temporary mint guard. |
| 4 | `WrapAlreadyExists` | A wrap record already exists for the `(user, period)` pair | Retrying the same mint, or attempting to mint twice for same user+period | 1) Check whether `get_wrap(user, period)` already returns a record. 2) If your UI retries, make the client idempotent. 3) If you intended a new wrap, use a new `period`. |
| 5 | `InvalidSignature` | Ed25519 signature verification failed against the contract's admin public key | - Wrong signature for the payload
- Wrong `contract_id` / payload fields
- Signature generated for a different user/period/archetype/data_hash
- **v1 payload submitted** — `CURRENT_PAYLOAD_VERSION` was bumped to 2 and old payloads are now rejected unconditionally | 1) Regenerate the signature using the correct canonical v2 payload (see `docs/signing-payload.md`), including the new `valid_until` field and `payload_version = 2`.
2) Confirm the signature corresponds to the correct contract instance (`contract_id` / `current_contract_address()`).
3) Confirm you sign for the correct `user`, `period`, `archetype`, `data_hash`, and `valid_until`.
4) Ensure you pass the 64-byte signature bytes (not base64/hex-decoded to the wrong length).
5) If you recently upgraded the contract, verify your backend migrated from payload v1 to v2. |
| 6 | `InvalidPeriod` | The period value is malformed or out of range | Period does not follow `YYYYMM` format (year 2024–2100, month 01–12) | Ensure `period` uses a valid year (2024–2100) and month (01–12). |
| 7 | `WrapNotFound` | A wrap record was not found for the `(user, period)` pair | Revoking a wrap that never existed, or period mismatch | 1) Use `get_wrap(user, period)` to confirm existence. 2) Ensure you are passing the exact same `period` value used when the wrap was minted. 3) If the record may have been revoked, mint again or fetch the correct period. |
| 57 | `SignatureExpired` | The mint signature's `valid_until` ledger timestamp has been exceeded (signature is no longer valid) | - The user submitted a mint transaction too slowly and the short-lived bearer signature expired
- A cached or leaked signature from a backend log/support ticket was replayed well after its intended lifetime
- `valid_until` was set too aggressively (e.g. 10 seconds, slower than the actual submit-to-ledger latency) | 1) Request a **fresh signature** from the backend signing service. The backend should issue signatures with `valid_until = now + T` where `T` covers the expected submit-to-ledger window plus some buffer (e.g. 15–30 minutes, not days).
2) If you are building a UI, surface this error as "Your link has expired. Please request a new one." rather than a generic crypto failure.
3) If you are the signing service, check that your clock is roughly in sync with the Stellar ledger (the contract compares `valid_until` against `e.ledger().timestamp()`, not wall-clock time on the signer).
4) For one-off manual ops, `u64::MAX` is a legal "never expires" value — avoid it for automated paths, but it is acceptable for backfills. |

---

## Variant → producing code path → failing-path test

Every variant in `ContractError` must be reachable from a code path and asserted by at least one failing-path test. This table is the source of truth for that mapping; keep it in sync when adding or removing variants.

| Code | Variant | Producing code path | Failing-path test |
|---:|---|---|---|
| 1 | `AlreadyInitialized` | `initialize()` guard when `DataKey::Admin` is already set | `test_initialize_twice_fails` (asserts `Error(Contract, #1)`) |
| 2 | `NotInitialized` | `require_initialized()` / admin lookup when `DataKey::Admin` is unset | `test_mint_before_initialize_fails` (asserts `Error(Contract, #2)`) |
| 3 | `Unauthorized` | `require_admin()` on admin-only entrypoints; `user.require_auth()` in `mint_wrap`; reentrancy guard | `test_non_admin_update_wrap_fails`, `test_mint_wrong_auth_fails` (assert `Error(Contract, #3)`) |
| 4 | `WrapAlreadyExists` | `mint_wrap` duplicate `(user, period)` check | `test_mint_duplicate_period_fails` (asserts `Error(Contract, #4)`) |
| 5 | `InvalidSignature` | `verify_ed25519` failure in `mint_wrap` | `test_mint_bad_signature_fails` (asserts `Error(Contract, #5)`) |
| 6 | `InvalidPeriod` | `validate_period()` range/format check | `test_mint_invalid_period_fails` (asserts `Error(Contract, #6)`) |
| 7 | `WrapNotFound` | `revoke_wrap` / `get_wrap` lookup miss | `test_revoke_missing_wrap_fails` (asserts `Error(Contract, #7)`) |

If a variant has no producing code path, it is dead surface and must be removed from `ContractError` (and from this table). If a variant is reachable but has no failing-path test, add one before merging.

---

## CI guard: new variants must ship with a test

To keep the mapping above from rotting, CI runs a check that fails when a variant is added to `ContractError` without a corresponding failing-path test. The check:

1. Parses the `ContractError` enum in `src/lib.rs` and collects every variant name.
2. Scans the test suite for each variant name (e.g. `ContractError::InvalidPeriod` or `Error(Contract, #6)`).
3. Fails the build if any variant is not referenced by at least one test.

When you add a variant, add its producing code path, a failing-path test that asserts it, and a row in the table above in the same PR.

---

## Example Soroban CLI output

Soroban typically reports contract panics as:

- `... Error(Contract, #N)`

Below are representative examples matching this repo’s tests.

### Code 1 — `AlreadyInitialized`
```text
thread 'main' panicked at 'Error(Contract, #1)'
```

### Code 2 — `NotInitialized`
```text
thread 'main' panicked at 'Error(Contract, #2)'
```

### Code 3 — `Unauthorized`
```text
thread 'main' panicked at 'Error(Contract, #3)'
```

### Code 4 — `WrapAlreadyExists`
```text
thread 'main' panicked at 'Error(Contract, #4)'
```

### Code 5 — `InvalidSignature`
```text
thread 'main' panicked at 'Error(Contract, #5)'
```

### Code 6 — `InvalidPeriod`
```text
thread 'main' panicked at 'Error(Contract, #6)'
```

### Code 7 — `WrapNotFound`
```text
thread 'main' panicked at 'Error(Contract, #7)'
```

---

## Payload & signing notes (for #6)

`mint_wrap` reconstructs a canonical payload as:

`0x01 ‖ contract_id ‖ user ‖ period ‖ archetype ‖ data_hash`

Then verifies an Ed25519 signature over that payload using the stored `admin_pubkey`. Old signatures generated without the version byte will fail verification after this migration.

Troubleshooting checklist for `Error(Contract, #6)`:
- Ensure `contract_id` in your signing process matches the deployed contract you are calling.
- Ensure `user` and `period` match the call parameters.
- Ensure `archetype` matches exactly (including symbol bytes).
- Ensure `data_hash` is sha256 of the exact off-chain bytes you used.

---

## Implicit panics & runtime behavior

Some failures can look like “unexpected panics” depending on the tooling:

- `ContractError::InvalidSignature` (`Error(Contract, #5)`) is raised whenever an Ed25519 mint signature fails to verify — wrong key, tampered payload/period, non-canonical public key, or a corrupted signature. The contract verifies signatures in-guest (`src/signature.rs`, `verify_ed25519`) so failures always surface as the contract error rather than a raw host `Crypto`/`InvalidInput` error.
- If you see `Error(Contract, #6)`, it corresponds to `ContractError::InvalidPeriod`, not a signature failure.

---

## Unstake / withdraw timing (unbonding period)

`unstake` and `withdraw_stake` are separate entry points with an unbonding delay between them. The following interleavings have defined, documented outcomes:

- **Stake again after `unstake`, before `withdraw_stake`:** the pending withdrawal is **not** reset, cancelled, or corrupted. The unbonding record (amount + unlock ledger) is preserved, and the new stake is tracked independently. `withdraw_stake` still releases only the originally unstaked amount once the delay elapses.
- **`withdraw_stake` before the delay elapses:** fails cleanly (the unlock ledger has not been reached); no funds are released.
- **`withdraw_stake` twice:** the second call fails cleanly because the pending withdrawal record is cleared on the first successful withdrawal; no double release.
- **`unstake` with no active stake:** fails cleanly rather than writing a zero record; no empty/zero withdrawal is persisted.
- **Withdrawing exactly at the boundary ledger:** succeeds — the delay is satisfied when the current ledger equals the unlock ledger.

Each of these interleavings is covered by a test.

---

## Troubleshooting tips (fast)

- If you see **`Error(Contract, #3)`**, check that:
  - You are calling admin-only functions (`update_admin`, `revoke_wrap`) from the **admin address** (or providing the correct authorization).
  - The `user` provided to `mint_wrap` is the same address that authorizes the call.
- If you see **`Error(Contract, #4)`**, check whether you already minted `(user, period)` with `get_wrap(user, period)`.
- If you see **`Error(Contract, #5)`**, verify your Ed25519 signature and canonical payload encoding.
- If you see **`Error(Contract, #6)`**, ensure `period` follows `YYYYMM` format (year 2024–2100, month 01–12).
- If you see **`Error(Contract, #7)`**, the wrap record does not exist. Use `get_wrap(user, period)` to confirm or mint a new wrap.

---

## Rustdoc cross-reference

These error codes are defined in `src/lib.rs` under:

- `/// Errors returned by the StellarWrap contract.` (`ContractError`)

---

## License

Same license as the rest of this repository.

## Error 57 — `StaleProposal`

**Variant**: `ContractError::StaleProposal`
**Code**: `57`
**Introduced**: Issue #864 — Governance proposal staleness fix.

### What it means

`execute_admin_proposal` detected that the contract admin has changed since the
proposal was created. The proposal's `admin_at_creation` field no longer matches
`DataKey::Admin`, which means executing the proposal would silently revert the
contract to a superseded admin state.

### Common causes

1. **Direct admin rotation**: `update_admin` was called between proposal creation
   and execution.
2. **Timelocked `SetAdmin`**: A `timelock_execute` for a `SetAdmin` action
   completed while the proposal was open.
3. **Concurrent proposal race**: A different governance proposal executed first,
   changing the admin, leaving this proposal stale.

### Resolution

The stale proposal cannot be executed. To change the admin, create a new
proposal under the current admin. The old stale proposal may be cancelled via
`cancel_admin_proposal` to clean up storage.
