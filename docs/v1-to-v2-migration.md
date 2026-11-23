# v1 → v2 Migration Guide

This guide helps frontend integrators, backend signing services, and indexer operators move from the v1 Stellar Wrap contract to v2. It covers the new contract address, changed and added entrypoints, updated error codes, event schema changes, and a recommended cutover timeline.

---

## Table of contents

1. [What changed at a glance](#1-what-changed-at-a-glance)
2. [Contract address](#2-contract-address)
3. [Entrypoint changes](#3-entrypoint-changes)
4. [Error code mapping](#4-error-code-mapping)
5. [Event schema changes](#5-event-schema-changes)
6. [Signing payload changes (backend)](#6-signing-payload-changes-backend)
7. [WrapRecord shape changes](#7-wraprecord-shape-changes)
8. [Indexer dual-read period](#8-indexer-dual-read-period)
9. [Cutover timeline](#9-cutover-timeline)
10. [Frontend integration checklist](#10-frontend-integration-checklist)
11. [Backend signer checklist](#11-backend-signer-checklist)
12. [Indexer checklist](#12-indexer-checklist)

---

## 1. What changed at a glance

| Area | v1 | v2 |
|---|---|---|
| Contract address | v1 contract ID | New v2 contract ID (see §2) |
| `mint_wrap` signature | No `payload_version`, no `valid_until` | Requires `payload_version = 2` and `valid_until` timestamp |
| `mint_wrap` payload | 5 fields | 6 fields (adds `valid_until`) |
| Error code 7 | `WrapNotFound` | `MigrationAlreadyApplied` (renumbered — see §4) |
| New error codes | — | `#8 InvalidStateTransition`, `#9 WrapNotFound`, `#10–#12` admin/pause errors |
| `WrapRecord` shape | `{timestamp, data_hash, archetype, period}` | Adds `fsm: WrapLifecycleFSM` field |
| `revoke_wrap` | Removed the wrap record | Still removes the record; emits `revoke` event with `reason_hash` |
| New entrypoints | — | `pause`, `is_paused`, `transition_wrap_state`, `get_wraps`, `get_mint_timestamp`, `total_wrap_count`, `set_alias_hash`, `get_alias_hash`, `propose_admin`, `accept_admin`, `cancel_proposed_admin`, `get_pending_admin`, `set_name`, `set_symbol`, `total_revoked`, `renew_all_ttls` |
| Admin transfer | `update_admin` (immediate) | Two-step: `propose_admin` → `accept_admin` (safe handoff) |
| Pause | No `pause` entrypoint | `pause` / `unpause` / `is_paused` |
| Indexer contract filter | v1 contract ID | v2 contract ID; see dual-read window in §8 |

---

## 2. Contract address

v2 is a **new deployment**, not an in-place upgrade of the v1 instance. The contract address (contract ID) changes.

Obtain the v2 contract ID from the release announcement or the deploy output of `make deploy-testnet` / `stellar contract deploy`. After cutover, replace every hardcoded occurrence of the v1 contract ID in:

- frontend environment variables (`CONTRACT_ID`, `VITE_CONTRACT_ID`, etc.)
- backend signing services (the `contract_address` field in the payload)
- indexer configuration (`CONTRACT_ID` environment variable)
- CLI scripts and CI jobs

> **Note on `initialize`:** v2 is initialized exactly once after deployment, identical to v1. Do not call `initialize` again on the v2 instance if you are pointing existing clients to it after the upgrade.

---

## 3. Entrypoint changes

### 3a. Changed signatures

#### `mint_wrap`

v1:
```
mint_wrap(user, period, archetype, data_hash, signature)
```

v2:
```
mint_wrap(user, period, archetype, data_hash, payload_version, valid_until, signature)
```

Two new required arguments:
- `payload_version: u32` — must be `2`. Passing `1` causes `Error(Contract, #5)` (`InvalidSignature`).
- `valid_until: u64` — Unix timestamp (seconds) after which the signature is rejected with `Error(Contract, #13)` (`SignatureExpired`). Set this to a reasonable future window (e.g. 10–15 minutes from signing time).

The signature is now computed over a 6-field payload (see §6).

---

#### `update_admin`

v1 allowed immediate admin replacement via `update_admin(new_admin)`.

v2 retains `update_admin` for direct replacement but adds a two-step safe handoff path:
1. Current admin calls `propose_admin(new_admin)`.
2. `new_admin` calls `accept_admin()` to take ownership.
3. Current admin can call `cancel_proposed_admin()` to abort.

Recommendation: use the two-step path for key rotations. `update_admin` still works but emits the same `admin / updated` event without requiring the new admin's confirmation.

---

#### `unpause`

Unchanged signature, but v2 also adds `pause(e)` and `is_paused(e) -> bool`. The `unpause` function internally calls the shared `set_pause(e, false)`.

---

### 3b. Removed entrypoints

| v1 entrypoint | v2 status | Replacement |
|---|---|---|
| `update_wrap` | **Removed** | Use `transition_wrap_state` to move a wrap through its lifecycle FSM |

`update_wrap` allowed arbitrary field updates to an existing wrap record. In v2 that is replaced by explicit state transitions via `transition_wrap_state(user, period, next_state)`. Allowed transitions are:

```
Draft     → Pending   | Cancelled
Pending   → Active    | Cancelled
Active    → Archived  | Cancelled
```

Any other transition panics with `Error(Contract, #8)` (`InvalidStateTransition`).

---

### 3c. New entrypoints

| Entrypoint | Description |
|---|---|
| `pause(e)` | Admin-only emergency halt. All mint/transition calls reject with `#12 Paused` while paused. |
| `is_paused(e) -> bool` | Read the current pause state. |
| `transition_wrap_state(user, period, next_state)` | Move a wrap through the lifecycle FSM. Requires the user's authorization. |
| `get_wraps(user, start, limit) -> Vec<WrapRecord>` | Paginated list of a user's wraps. |
| `get_mint_timestamp(user, period) -> Option<u64>` | Ledger timestamp of a specific mint. |
| `total_wrap_count() -> u32` | Global count of all successful mints. |
| `set_alias_hash(user, alias_hash)` | Store a 32-byte alias hash for the caller. |
| `get_alias_hash(user) -> Option<BytesN<32>>` | Retrieve a user's alias hash. |
| `propose_admin(new_admin)` | Begin two-step admin transfer. |
| `accept_admin()` | Complete two-step admin transfer (called by proposed admin). |
| `cancel_proposed_admin()` | Abort a pending admin transfer proposal. |
| `get_pending_admin() -> Option<Address>` | Read the pending admin address if a proposal is active. |
| `set_name(name)` / `set_symbol(symbol)` | Admin-override for the token display name and symbol. |
| `total_revoked() -> u64` | Global count of revoked wraps. |
| `renew_all_ttls(user)` | Admin-only bulk TTL renewal for a user's metadata keys. |

---

## 4. Error code mapping

The numeric codes changed between v1 and v2. If your code matches on raw error numbers (e.g. from CLI output or RPC error strings), update every match.

| Code | v1 variant | v2 variant | Notes |
|---:|---|---|---|
| 1 | `AlreadyInitialized` | `AlreadyInitialized` | Unchanged |
| 2 | `NotInitialized` | `NotInitialized` | Unchanged |
| 3 | `Unauthorized` | `Unauthorized` | Unchanged |
| 4 | `WrapAlreadyExists` | `WrapAlreadyExists` | Unchanged |
| 5 | `InvalidSignature` | `InvalidSignature` | v2 also triggers this when `payload_version != 2` |
| 6 | `InvalidPeriod` | `InvalidPeriod` | Unchanged |
| 7 | `WrapNotFound` | `MigrationAlreadyApplied` | **Breaking change** — code 7 is now a different error |
| 8 | _(unused)_ | `InvalidStateTransition` | New in v2 |
| 9 | _(unused)_ | `WrapNotFound` | Moved from code 7 |
| 10 | _(unused)_ | `NoAdminTransferProposal` | New in v2 |
| 11 | _(unused)_ | `AdminTransferProposalExists` | New in v2 |
| 12 | _(unused)_ | `Paused` | New in v2 |
| 13 | _(unused)_ | `SignatureExpired` | New in v2 — `valid_until` check failed |

**Action required:** Any code that parses `Error(Contract, #7)` as "wrap not found" must be updated to check `#9` instead.

---

## 5. Event schema changes

### Mint event — unchanged structure, new contract ID filter

The mint event topic/data shape is the same in v2:

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"mint"` |
| Topic 1 | `Address` | user address |
| Topic 2 | `u64` | period (YYYYMM) |
| Data | `Symbol` | archetype |

The only change is the emitting contract address. Update your indexer's `contractIds` filter to the v2 contract ID.

---

### Transition event — new in v2

Emitted by `transition_wrap_state`:

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"trans"` |
| Topic 1 | `Address` | user address |
| Topic 2 | `u64` | period (YYYYMM) |
| Data | `WrapState` | the new state (enum value) |

---

### Revoke event — `reason_hash` now included

v1 revoke event emitted just `(user, period)`. v2 adds `reason_hash` as the event data:

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"revoke"` |
| Topic 1 | `Address` | user address |
| Topic 2 | `u64` | period (YYYYMM) |
| Data | `BytesN<32>` | SHA-256 of the off-chain revocation reason (all-zero if omitted) |

---

### Admin update event — unchanged

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"admin"` |
| Topic 1 | `Symbol` | `"updated"` |
| Data | `(Address, Address)` | `(old_admin, new_admin)` |

---

### Pause event — new in v2

Emitted by `pause` and `unpause`:

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"pause"` |
| Data | `bool` | `true` when paused, `false` when unpaused |

---

### Init event — new in v2

Emitted by `initialize`:

| Field | Type | Value |
|---|---|---|
| Topic 0 | `Symbol` | `"init"` |
| Data | `Address` | admin address set during initialization |

---

## 6. Signing payload changes (backend)

### v1 payload (5 fields)

```
payload = XDR(contract_address)
        ‖ XDR(user_address)
        ‖ XDR(period)
        ‖ XDR(archetype)
        ‖ XDR(data_hash)
```

### v2 payload (6 fields)

```
payload = XDR(contract_address)
        ‖ XDR(user_address)
        ‖ XDR(period)
        ‖ XDR(archetype)
        ‖ XDR(data_hash)
        ‖ XDR(valid_until)    ← new field
```

`valid_until` is a `u64` Unix timestamp encoded as XDR `Uint64`. It must match the value passed to `mint_wrap` exactly.

The contract also validates `payload_version == 2`. Signatures generated with the v1 layout (or with `payload_version = 1`) are rejected.

### Required backend changes

1. Add `valid_until` to the signing context. Recommended: `ledger_timestamp + 900` (15 minutes).
2. Append `XDR(valid_until)` as the sixth field in the payload byte string.
3. Pass `payload_version = 2` and `valid_until` as arguments to `mint_wrap`.
4. Update the `contract_address` field in your signing configuration to the v2 contract ID.

See [docs/signing-payload.md](signing-payload.md) for the full field encoding reference.

---

## 7. WrapRecord shape changes

v1 `WrapRecord`:
```rust
struct WrapRecord {
    timestamp: u64,
    data_hash: BytesN<32>,
    archetype: Symbol,
    period: u64,
}
```

v2 `WrapRecord` adds a lifecycle FSM:
```rust
struct WrapRecord {
    timestamp: u64,
    data_hash: BytesN<32>,
    archetype: Symbol,
    period: u64,
    fsm: WrapLifecycleFSM,  // new field
}
```

`WrapLifecycleFSM` contains:
```rust
struct WrapLifecycleFSM {
    state: WrapState,   // enum: Draft=1, Pending=2, Active=3, Archived=4, Cancelled=5
    updated_at: u64,    // ledger timestamp of last transition
}
```

New wraps are created in the `Active` state. Clients that deserialize `WrapRecord` by field position (rather than by name via XDR) must add the `fsm` field to avoid deserialization errors.

---

## 8. Indexer dual-read period

Because v2 is a new contract deployment, historical mints from v1 do not exist in v2 storage. During the cutover window, your indexer should read from both contract IDs.

### Recommended dual-read strategy

1. **Before cutover:** continue indexing only the v1 contract.
2. **At deploy time:** start indexing the v2 contract alongside v1 from its first ledger.
3. **During overlap (recommended minimum: 7 days):** serve queries from both contracts — union the results, deduplicating by `(user, period)`. v2 results take precedence for any overlapping records.
4. **After deprecation:** stop subscribing to v1 events. Keep the historical v1 data in your database for audit purposes.

### Schema change

The `mint_events` table does not need structural changes (user, period, archetype, ledger_seq are unchanged). Add a `contract_version` column to distinguish v1 from v2 records during the overlap window:

```sql
ALTER TABLE mint_events ADD COLUMN contract_version INTEGER NOT NULL DEFAULT 1;
```

Set `contract_version = 2` for all events indexed from the v2 contract ID.

### New events to index in v2

| Event | Topics | Data | Action |
|---|---|---|---|
| `trans` | `["trans", user, period]` | `WrapState` | Record wrap lifecycle transitions |
| `revoke` | `["revoke", user, period]` | `BytesN<32>` (reason hash) | Mark wrap as revoked; store reason hash |
| `pause` | `["pause"]` | `bool` | Track contract pause/unpause for SLA monitoring |

---

## 9. Cutover timeline

| Phase | Action | Who |
|---|---|---|
| **Freeze** | Stop minting on v1. Announce freeze date to users. | Product / Admin |
| **Deploy** | Deploy v2 contract; run `initialize` once; call `migrate(1)` to mark the initial migration version. | Ops / Admin |
| **Backend update** | Update signing service: new contract ID, v2 payload (6 fields), `valid_until`, `payload_version = 2`. | Backend |
| **Frontend update** | Update `CONTRACT_ID` env var; update any `WrapRecord` deserialization to handle the new `fsm` field; update error code handling (#7 → #9 for `WrapNotFound`). | Frontend |
| **Indexer update** | Add v2 contract ID to indexer filter; start dual-read period; add `contract_version` column; subscribe to `trans`, `revoke`, `pause` events. | Data / Infra |
| **Switch** | Direct all new traffic to v2. Keep v1 in read-only mode (do not send new mints). | Ops |
| **Deprecate** | After dual-read window (minimum 7 days), remove v1 mint traffic. Retain v1 historical data. | Ops / Data |

---

## 10. Frontend integration checklist

- [ ] Replace `CONTRACT_ID` with the v2 contract address in all environment configs.
- [ ] Update `mint_wrap` calls to include `payload_version` (value: `2`) and `valid_until`.
- [ ] Update `WrapRecord` deserialization / type definitions to include the `fsm` field.
- [ ] Update error handling: `Error(Contract, #7)` is now `MigrationAlreadyApplied`; `WrapNotFound` is `#9`.
- [ ] Handle new error code `#12 Paused` — show a maintenance message when the contract is paused.
- [ ] Handle new error code `#13 SignatureExpired` — prompt the user to retry with a fresh signature.
- [ ] Optionally surface wrap lifecycle state (`fsm.state`) in the UI: `Active`, `Archived`, `Cancelled`.
- [ ] Test against the v2 testnet deployment before switching mainnet traffic.

---

## 11. Backend signer checklist

- [ ] Update the target contract address to the v2 contract ID.
- [ ] Add `valid_until` to the signing context (recommended: 15 minutes from request time).
- [ ] Append `XDR(valid_until)` as field 6 in the payload byte string.
- [ ] Set `payload_version = 2` in all `mint_wrap` calls.
- [ ] Confirm the signing key (`admin_pubkey`) registered in the v2 contract matches your private key.
- [ ] Remove any code that generates v1 payloads (5-field layout); these will be rejected.
- [ ] Add monitoring for `#13 SignatureExpired` errors — these indicate clock skew or slow mint submission.

---

## 12. Indexer checklist

- [ ] Add the v2 contract ID to the `contractIds` filter.
- [ ] Add `contract_version` column to `mint_events` (or equivalent table).
- [ ] Record the starting ledger of the v2 deployment to avoid a gap.
- [ ] Subscribe to the `trans` event to track lifecycle transitions.
- [ ] Subscribe to the `revoke` event to mark revoked wraps.
- [ ] Subscribe to the `pause` event for SLA monitoring.
- [ ] Implement dual-read: union v1 and v2 results during the overlap window, with v2 taking precedence.
- [ ] After the deprecation date, stop subscribing to v1 events (retain historical data).
- [ ] Verify `balance_of(user)` and `total_wrap_count()` against your indexed counts as a consistency check.

---

## See also

- [ERRORS.md](../ERRORS.md) — full error catalogue with resolution steps
- [docs/signing-payload.md](signing-payload.md) — canonical payload encoding and test vectors
- [STORAGE.md](../STORAGE.md) — storage architecture and TTL strategy
- [CHANGELOG.md](../CHANGELOG.md) — version history
- [MAINNET_RELEASE_CHECKLIST.md](../MAINNET_RELEASE_CHECKLIST.md) — deployment checklist
