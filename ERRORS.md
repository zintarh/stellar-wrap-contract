# Error Reference (ContractError) — Stellar Wrap

This document maps the on-chain contract error codes surfaced by Soroban to their meaning and fixes.

Soroban surfaces contract panics as an `Error(Contract, #N)` string in transaction results.

The codes are defined by the Rust `ContractError` enum in `src/errors.rs`.
`src/error_messages.json` is the single source of truth for code → user-facing message; `src/error_messages.rs` and `scripts/generate_errors_md.py --check` (CI) keep every copy in sync.

---

## Quick table: error code → meaning

Canonical source: `src/error_messages.json` (code, variant, actionable flag, message).
The table below is generated — run `python3 scripts/generate_errors_md.py` after editing the JSON.

<!-- BEGIN GENERATED TABLE: do not edit by hand; run scripts/generate_errors_md.py -->

| Code | Variant name | User can act? | Message |
|---:|---|:---:|---|
| 1 | `AlreadyInitialized` | no | This contract is already initialized. |
| 2 | `NotInitialized` | no | This contract has not been initialized. |
| 3 | `Unauthorized` | no | This account is not allowed to perform that action. |
| 4 | `WrapAlreadyExists` | yes | This period is already minted for this account. Open the existing wrap, or mint a different period. |
| 5 | `InvalidSignature` | yes | The mint signature was rejected. Request a new signature for this account, period, and data hash, then try again. |
| 6 | `InvalidPeriod` | yes | That period is not valid. Use a YYYYMM month from 2024 through 2100. |
| 7 | `MigrationAlreadyApplied` | no | That migration has already been applied. |
| 8 | `InvalidStateTransition` | no | The wrap record cannot move to that state. |
| 9 | `WrapNotFound` | yes | No wrap exists for this account and period. Check the period and try again. |
| 10 | `NoAdminTransferProposal` | no | There is no admin transfer waiting to be accepted. |
| 11 | `AdminTransferProposalExists` | no | An admin transfer is already waiting. |
| 12 | `Paused` | yes | The contract is paused. Try again after it is unpaused. |
| 13 | `ArithmeticOverflow` | no | The contract hit an arithmetic overflow. |
| 14 | `InvalidFeeParams` | no | The fee parameters are not valid. |
| 15 | `BatchEmpty` | yes | The batch is empty. Add at least one wrap and try again. |
| 16 | `BatchTooLarge` | yes | The batch is too large. Split it into smaller batches and try again. |
| 17 | `DuplicateBatchEntry` | yes | The batch lists the same account and period more than once. Remove the duplicate and try again. |
| 18 | `StakeTooLow` | yes | The stake amount is below the minimum. Stake a larger amount. |
| 19 | `StakeNotFound` | yes | There is no stake for this account. Stake before using this action. |
| 20 | `StakeCooldownActive` | yes | A stake cooldown is already active. Wait for it to finish. |
| 21 | `StakeNotUnstaking` | yes | This stake is not unstaking. Start unstaking before withdrawing. |
| 22 | `StakeCooldownNotElapsed` | yes | The stake cooldown has not finished. Wait until it elapses, then try again. |
| 23 | `InvalidStakeConfig` | no | The stake configuration is not valid. |
| 24 | `StakeArithmeticOverflow` | no | The contract hit a stake arithmetic overflow. |
| 25 | `ProposalNotFound` | yes | No proposal exists with that id. Check the id and try again. |
| 26 | `ProposalNotActive` | no | That proposal is not active. |
| 27 | `ProposalAlreadyVoted` | yes | This account has already voted on that proposal. |
| 28 | `ProposalVotingPeriodNotEnded` | yes | Voting is still open. Wait until the voting period ends. |
| 29 | `ProposalVotingPeriodEnded` | yes | The voting period for that proposal has ended. |
| 30 | `ProposalDefeated` | no | That proposal was defeated. |
| 31 | `InvalidProposalDuration` | no | The proposal duration is not valid. |
| 32 | `UserOptedOut` | yes | This account has opted out of new wraps. Opt back in before minting. |
| 33 | `BridgeNotInitialized` | no | The bridge is not initialized. |
| 34 | `InvalidChain` | yes | That chain id is not valid. Choose a supported chain. |
| 35 | `ChainDisabled` | yes | That chain is disabled. Choose a different chain. |
| 36 | `NonceAlreadyProcessed` | yes | That bridge nonce was already processed. Do not submit it again. |
| 37 | `InvalidBridgePayload` | yes | The bridge payload is not valid. Check it and try again. |
| 38 | `MerkleRootNotSet` | no | No whitelist root is set. |
| 39 | `InvalidMerkleProof` | yes | This account is not on the whitelist. Ask to be added, or submit a valid whitelist proof. |
| 40 | `TimelockNotReady` | no | That timelocked operation is not ready yet. |
| 41 | `TimelockOperationNotFound` | no | That timelocked operation does not exist. |
| 42 | `TimelockOperationExists` | no | That timelocked operation is already queued. |
| 43 | `InvalidTimelockDelay` | no | The timelock delay is not valid. |
| 44 | `TimelockRequired` | no | That action has to go through the timelock. |
| 45 | `TimelockAlreadyEnabled` | no | The timelock is already enabled. |
| 46 | `WrapNotExpired` | yes | This wrap has not expired yet. Wait until it expires, then try again. |
| 47 | `InvalidExpirationDuration` | no | The expiration duration is not valid. |
| 48 | `TransferFeeAlreadyConfigured` | no | The transfer fee has already been configured. |
| 49 | `InvalidTransfer` | yes | That transfer is not valid. Check the recipient and try again. |
| 50 | `TransferInProgress` | yes | A transfer for this wrap is already in progress. Wait for it to finish. |
| 51 | `StorageInvariantViolation` | no | The contract found a storage invariant violation. |
| 52 | `InvalidAdminPubKey` | no | The admin signing key is not valid. |
| 53 | `InvalidThreshold` | no | The threshold is not valid. |
| 54 | `MerkleProofTooLong` | yes | The whitelist proof is too long. Submit a shorter proof. |
| 55 | `TimelockOperationExpired` | no | That timelocked operation has expired. |
| 56 | `TimelockOperationNotExpired` | no | That timelocked operation has not expired yet. |
| 57 | `BridgeRequestNotPending` | no | That bridge request is not pending, so it cannot be refunded. |
| 58 | `BridgeRefundDelayNotElapsed` | yes | The bridge refund delay has not elapsed yet. Wait until the unlock time, then try again. |
| 59 | `BridgeRequestAlreadyRefunded` | no | That bridge request has already been refunded. |
| 60 | `StaleProposal` | yes | The admin changed after this proposal was created. Create a new proposal under the current admin. |
| 61 | `InvalidRelayerSet` | no | The relayer set is not valid. It must meet the threshold and contain no duplicates. |
| 62 | `SignatureExpired` | yes | The mint signature has expired. Request a new signature with a later valid_until, then try again. |

Generated from `src/error_messages.json` (62 variants). Do not edit this table by hand: run `python3 scripts/generate_errors_md.py` and commit the result.

<!-- END GENERATED TABLE -->
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

## Payload & signing notes (for `InvalidSignature`, code 5)

`mint_wrap` reconstructs a canonical payload as:

`0x01 ‖ contract_id ‖ user ‖ period ‖ archetype ‖ data_hash`

Then verifies an Ed25519 signature over that payload using the stored `admin_pubkey`. Old signatures generated without the version byte will fail verification after this migration.

Troubleshooting checklist for `Error(Contract, #5)`:
- Ensure `contract_id` in your signing process matches the deployed contract you are calling.
- Ensure `user` and `period` match the call parameters.
- Ensure `archetype` matches exactly (including symbol bytes).
- Ensure `data_hash` is sha256 of the exact off-chain bytes you used.

---

## Implicit panics & runtime behavior

Some failures can look like “unexpected panics” depending on the tooling:

- `ContractError::InvalidSignature` (`Error(Contract, #5)`) is raised whenever an Ed25519 mint signature fails to verify — wrong key, tampered payload/period, non-canonical public key, or a corrupted signature. The contract verifies signatures in-guest (`src/signature.rs`, `verify_ed25519`) so failures always surface as the contract error rather than a raw host `Crypto`/`InvalidInput` error.
- If you see `Error(Contract, #5)`, it corresponds to `ContractError::InvalidSignature`.
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

## Error 60 — `StaleProposal`

**Variant**: `ContractError::StaleProposal`
**Code**: `60`
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
