# Timelock controller for administrative actions

Privileged actions on this contract used to take effect in the same transaction
that requested them: a compromised or careless admin key could hand over the
admin role, rotate the signing key, or swap the contract WASM with no warning.
The timelock controller ([`src/timelock.rs`](../src/timelock.rs)) puts a
mandatory, publicly observable waiting period in front of those actions.

## Architecture

```
enable_timelock(delay)      one-way switch, admin only
        │
        ▼
timelock_schedule(action) ──► TimelockOp(id) { action, eta, scheduled_at }
        │                              │
        │ eta = now + delay            │  observable on-chain + event
        ▼                              ▼
timelock_execute(id)  ◄── when eta <= ledger timestamp <= eta + GRACE_PERIOD
timelock_cancel(id)   ◄── admin may drop it at any point before execution
timelock_sweep_expired(id) ◄── anyone can sweep once timestamp > eta + GRACE_PERIOD
```

Two pieces of state, both introduced in `DataKey`:

- `TimelockDelay` (instance) — the delay in seconds. **Its presence is what
  enables the timelock.** Absent ⇒ controller disabled.
- `TimelockOp(id)` (persistent, ~1 year TTL) — one scheduled operation.
  `TimelockOps` (instance) holds the id list for enumeration.

### Operation ids

An id is `SHA256(variant_tag || XDR(payload))` — deterministic and independent
of the ETA. Two consequences:

- The same action cannot be queued twice concurrently
  (`TimelockOperationExists`), so the queue can't be spammed with duplicates.
- Off-chain tooling can pre-compute the id it will later need to execute;
  `timelock_operation_id(action)` exposes the same computation as a view.

### Actions

`TimelockAction` is a closed enum, so a scheduled operation can never invoke
something the contract does not already expose:

| Variant | Effect on execute |
| --- | --- |
| `SetAdmin(address)` | Replaces `Admin`, clears any `PendingAdmin`, emits `("admin", "updated")`. |
| `SetAdminPubKey(bytes32)` | Rotates the Ed25519 mint-signing key. |
| `Upgrade(wasm_hash)` | Emits `("upgrade",)` then `update_current_contract_wasm`. |
| `SetWhitelistRoot(bytes32)` | Publishes a new whitelist merkle root. |
| `SetTimelockDelay(seconds)` | Changes the delay itself. |

The delay is bounded to `MIN_DELAY` (1 hour) … `MAX_DELAY` (30 days).
`InvalidTimelockDelay` is raised at *schedule* time as well as at execute time,
so an out-of-range value can never sit in the queue waiting to brick the
controller.

### Grace Period and Expiration

Scheduled operations carry a mandatory grace period of **14 days** (`GRACE_PERIOD = 1,209,600` seconds).
- **Valid execution window:** `op.eta <= ledger.timestamp <= op.eta + GRACE_PERIOD`.
- **Expired operations:** If `ledger.timestamp > op.eta + GRACE_PERIOD`, calling `timelock_execute` will panic with `TimelockOperationExpired` (66).
- **Sweeping expired operations:** Anyone can call `timelock_sweep_expired(id)` once an operation is expired. This removes the operation from storage and `TimelockOps` list so `timelock_pending` stays truthful.

## What the timelock closes off

Once enabled, the direct paths panic with `TimelockRequired` (44):

- `update_admin`
- `propose_admin` and `accept_admin` — the two-step handover is also blocked,
  because a proposal that can be accepted immediately would bypass the delay.
  `cancel_proposed_admin` stays open so a stale proposal can still be cleared.
- `upgrade`

Deliberately **not** timelocked: `pause` / `unpause`. An emergency stop is only
useful if it is immediate, and pausing cannot move value or change ownership.

## Guarantees and caveats

- **One-way switch.** `enable_timelock` can be called once
  (`TimelockAlreadyEnabled`). There is no disable path; lengthening or
  shortening the delay is itself a timelocked action, so any weakening of the
  protection is announced by the same delay it is trying to weaken.
- **Execution is not automatic.** After the ETA passes, the admin must still
  call `timelock_execute`. A queued operation is removed from storage *before*
  its effect is applied, so it can never be replayed.
- **`eta` uses ledger timestamps**, not wall clock; treat the delay as
  approximate to within normal ledger-close drift.
- **Cancellation is admin-only.** The timelock buys observers time to react
  (withdraw, alert, fork); it does not give them a veto.
- Existing deployments are unaffected until `enable_timelock` is called, so this
  is a backwards-compatible addition.

## Operator runbook

```bash
# 1. Turn it on with a 48-hour delay (one-way).
enable_timelock --delay_seconds 172800

# 2. Queue an admin handover; note the returned id (or pre-compute it with
#    timelock_operation_id).
timelock_schedule --action '{"SetAdmin":"G..."}'

# 3. Anyone can audit the queue while the clock runs.
timelock_pending
timelock_operation --id <id>     # -> { action, eta, scheduled_at }

# 4. After eta (and within eta + 14 days), apply it.
timelock_execute --id <id>

# Abort instead, at any time before step 4:
timelock_cancel --id <id>

# Sweep an expired operation (> eta + 14 days):
timelock_sweep_expired --id <id>
```

## Errors

| Code | Error | Meaning |
| --- | --- | --- |
| 40 | `TimelockNotReady` | ETA not reached. |
| 41 | `TimelockOperationNotFound` | Unknown or already-executed id. |
| 42 | `TimelockOperationExists` | Identical action already queued. |
| 43 | `InvalidTimelockDelay` | Delay out of bounds, or timelock not enabled. |
| 44 | `TimelockRequired` | Direct admin call attempted while enabled. |
| 45 | `TimelockAlreadyEnabled` | `enable_timelock` called twice. |
| 66 | `TimelockOperationExpired` | Attempted to execute past `eta + GRACE_PERIOD`. |
| 67 | `TimelockOperationNotExpired` | Attempted to sweep before `eta + GRACE_PERIOD`. |
