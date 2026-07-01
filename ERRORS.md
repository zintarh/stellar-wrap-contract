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
| 5 | `WrapNotFound` | A wrap record was not found for the `(user, period)` pair | Revoking/updating a wrap that never existed, or period mismatch | 1) Use `get_wrap(user, period)` to confirm existence. 2) Ensure you are passing the exact same `period` value used when the wrap was minted. 3) If the record may have been revoked, mint again or fetch the correct period. |
| 6 | `InvalidSignature` | Ed25519 signature verification failed against the contract’s admin public key | - Wrong signature for the payload
- Wrong `contract_id` / payload fields
- Signature generated for a different user/period/archetype/data_hash | 1) Regenerate the signature using the correct canonical payload (see “Payload & signing notes” below).
2) Confirm the signature corresponds to the correct contract instance (`contract_id` / `current_contract_address()`).
3) Confirm you sign for the correct `user`, `period`, `archetype`, and `data_hash`.
4) Ensure you pass the 64-byte signature bytes (not base64/hex-decoded to the wrong length). |
| 7 | `InvalidDataHash` | `data_hash` is all-zero bytes (missing/invalid data) | Passing `0x00…00` as `data_hash` | 1) Compute `data_hash = sha256(original_json_bytes)`.
2) Ensure you don’t initialize `data_hash` with a zero placeholder.
3) Validate the off-chain JSON bytes are the same bytes you intend to mint. |
| 8 | `UserOptedOut` | The user has opted out of receiving future wraps | Calling `mint_wrap` or `mint_campaign_wrap` for an opted-out user | 1) Ask the user to call `opt_in()` to re-enable mints.
2) Check opt-out status before attempting to mint. |

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

### Code 5 — `WrapNotFound`
```text
thread 'main' panicked at 'Error(Contract, #5)'
```

### Code 6 — `InvalidSignature`
```text
thread 'main' panicked at 'Error(Contract, #6)'
```

### Code 7 — `InvalidDataHash`
```text
thread 'main' panicked at 'Error(Contract, #7)'
```

### Code 8 — `UserOptedOut`
```text
thread 'main' panicked at 'Error(Contract, #8)'
```

---

## Payload & signing notes (for #6)

`mint_wrap` reconstructs a canonical payload as:

`contract_id ‖ user ‖ period ‖ archetype ‖ data_hash`

Then verifies an Ed25519 signature over that payload using the stored `admin_pubkey`.

Troubleshooting checklist for `Error(Contract, #6)`:
- Ensure `contract_id` in your signing process matches the deployed contract you are calling.
- Ensure `user` and `period` match the call parameters.
- Ensure `archetype` matches exactly (including symbol bytes).
- Ensure `data_hash` is sha256 of the exact off-chain bytes you used.

---

## Implicit panics & runtime behavior

Some failures can look like “unexpected panics” depending on the tooling:

- `ContractError::InvalidSignature` is raised when `env.crypto().ed25519_verify(...)` fails.
- If you see `Error(Contract, #6)`, it corresponds to `ContractError::InvalidSignature`.

If your CLI/tooling shows a different wording around an Ed25519 verify failure, still map it to code **#6** using the contract error.

---

## Troubleshooting tips (fast)

- If you see **`Error(Contract, #3)`**, check that:
  - You are calling admin-only functions from the **admin address** (or providing the correct authorization).
  - The `user` provided to `mint_wrap` is the same address that authorizes the call.
  - You’re not issuing concurrent/repeated invocations that trip the temporary mint guard.
- If you see **`Error(Contract, #4)`**, check whether you already minted `(user, period)` with `get_wrap(user, period)`.
- If you see **`Error(Contract, #5)`**, verify `period` matches the minted period exactly.
- If you see **`Error(Contract, #8)`**, the user has opted out of receiving wraps. Ask them to call `opt_in()` to re-enable mints.

---

## Rustdoc cross-reference

These error codes are defined in `src/lib.rs` under:

- `/// Errors returned by the StellarWrap contract.` (`ContractError`)

---

## License

Same license as the rest of this repository.

