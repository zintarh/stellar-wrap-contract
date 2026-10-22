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
timelock_execute(id)  ◄── only when ledger timestamp >= eta
timelock_cancel(id)   ◄── admin may drop it at any point before execution
timelock_sweep_expired(id) ◄── anyone may remove expired ops (now > eta + GRACE_PERIOD)
```

Two pieces of state, both introduced in `DataKey`:

- `TimelockDelay` (instance) — the delay in seconds. **Its presence is what
  enables the timelock.** Absent ⇒ controller disabled.
- `TimelockOp(id)` (persistent, ~1 year TTL) — one scheduled operation.
  `TimelockOps` (instance) holds the id list for enumeration.

### Grace period

Every scheduled operation has a **grace period** of `GRACE_PERIOD` (14 days,
1,209,600 seconds) after its ETA. The operation must be executed within this
window:

```
scheduled_at ──delay──► ETA ──GRACE_PERIOD──► expiry
                            │
                            ├── execute()   (eta ≤ now ≤ expiry)
                            └── sweep_expired()  (now > expiry)
```

- `timelock_execute` succeeds only while `now ≤ eta + GRACE_PERIOD`.
- After `now > eta + GRACE_PERIOD` the operation is **expired** and can no
  longer be executed. It remains in storage until someone calls
  `timelock_sweep_expired` to remove it from the pending list.
- The grace period bounds the execution window, preventing stale operations
  (e.g. a `SetAdmin` to a retired key, or an `Upgrade` to a superseded WASM
  hash) from being executed months later by an unsuspecting admin.

The constants are defined in [`src/timelock.rs`](../src/timelock.rs):
- `MIN_DELAY = 1 hour`
- `MAX_DELAY = 30 days`
- `GRACE_PERIOD = 14 days`

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

## Privileged entrypoint coverage

Once enabled, entrypoints marked **Yes** reject direct calls with
`TimelockRequired` and must be reached through `timelock_schedule` plus
`timelock_execute`. Entry points marked **No** remain direct admin calls by
design and are not represented by a `TimelockAction` variant.

| Entrypoint | Timelocked? | Rationale |
| --- | --- | --- |
| `initialize` | No | Deployment bootstrap is single-use and must be signed by the configured admin account. |
| `update_admin` | Yes | Ownership changes need an observable delay. |
| `propose_admin` / `accept_admin` | Yes | Two-step handover must not bypass the delay. |
| `cancel_proposed_admin` | No | Clearing a stale handover does not grant access or change ownership. |
| `upgrade` | Yes | WASM changes need an observable delay. |
| `set_whitelist_root` / `clear_whitelist_root` | Yes | Whitelist access-control changes need an observable delay. |
| `TimelockAction::SetAdminPubKey` | Yes | Mint-signing key rotation needs an observable delay; it has no direct entrypoint. |
| `migrate` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `set_name` / `set_symbol` | No | No corresponding timelock action exists; retained as direct admin metadata configuration. |
| `backfill_wrap_periods` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `pause` / `unpause` | No | An emergency stop must remain immediately available. |
| `set_transfer_fee` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_expiration_duration` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_fee_params` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_stake_config` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_bridge_relayer` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_chain_status` | No | No corresponding timelock action exists; retained as a direct admin configuration. |

The **No** entries are an explicit scope decision: they remain admin-only, but
the current closed `TimelockAction` enum provides no delayed operation for them.

## Governance and timelock interaction

Governance (`execute_admin_proposal`) and the timelock are two overlapping
paths to the same privileged action — changing the admin. Their interaction is
specified here and covered by tests in
[`src/timelock_test.rs`](../src/timelock_test.rs).

### Does a governance proposal execute immediately?

It depends on whether the timelock is enabled, and this is explicit in code:

- **Timelock disabled** (`TimelockDelay` absent): a passing proposal executes
  immediately. `execute_admin_proposal` sets `Admin` in the same transaction.
- **Timelock enabled** (`TimelockDelay` present): a passing proposal does **not**
  execute immediately. The current admin must authorize execution, and the
  passing proposal queues `TimelockAction::SetAdmin`; the admin remains
  unchanged until that queued operation reaches its ETA and is executed via
  `timelock_execute`.

So governance never bypasses the timelock: when the timelock is on, the
proposal's effect is routed through the same delay as a direct admin handover.

### Can a timelocked action change the admin while a proposal is open?

Yes, and it is not a bypass. A `SetAdmin` scheduled through the timelock is an
independent, admin-authorized operation. If it executes while a governance
proposal to change the admin is still open, it simply replaces `Admin` and
clears any `PendingAdmin`; the open proposal is then evaluated against the new
admin. Both paths require the current admin's authorization, so neither can
silently override the other without the admin's involvement.

### Can governance schedule, cancel, or shorten a timelocked action?

No. Governance has no entrypoint that touches the timelock queue:

- **Schedule** — only `timelock_schedule` (admin-only) queues operations.
  `execute_admin_proposal` may *cause* a `SetAdmin` to be queued when the
  timelock is enabled, but it cannot schedule arbitrary actions.
- **Cancel** — only `timelock_cancel` (admin-only) removes a queued operation.
- **Shorten** — the delay can only be changed by
  `TimelockAction::SetTimelockDelay`, which is itself subject to the current
  delay. Governance cannot shorten it.

## Full authority model

This section is the single place that answers "who can change what, and how
fast" across every privileged route. Each privileged action appears exactly
once, with all of the routes that reach it.

### Routes

| Route | Who may initiate | Delay | Who can cancel |
| --- | --- | --- | --- |
| **Admin direct** | Current `Admin` | None (immediate) | n/a |
| **Timelock** | Current `Admin` (`timelock_schedule`) | `TimelockDelay` (1h–30d) | Current `Admin` (`timelock_cancel`) |
| **Governance proposal** | Any token holder meeting the proposal threshold | Voting period, then (if timelock enabled) `TimelockDelay` | Admin can cancel the queued `SetAdmin` via `timelock_cancel`; the proposal itself is not cancellable once passed |
| **Bridge relayer** | Address registered via `set_bridge_relayer` | None (immediate) | Current `Admin` (by rotating the relayer) |

### Privileged actions and their routes

| Privileged action | Admin direct | Timelock | Governance | Bridge relayer | Fastest path |
| --- | --- | --- | --- | --- | --- |
| Change `Admin` (`update_admin` / `propose_admin`+`accept_admin`) | Yes | Yes (`SetAdmin`) | Yes (`execute_admin_proposal`) | No | Admin direct (immediate) |
| Rotate mint-signing key (`SetAdminPubKey`) | No | Yes | No | No | Timelock (1h–30d) |
| Upgrade WASM (`upgrade`) | No | Yes (`Upgrade`) | No | No | Timelock (1h–30d) |
| Set/clear whitelist root (`set_whitelist_root` / `clear_whitelist_root`) | No | Yes (`SetWhitelistRoot`) | No | No | Timelock (1h–30d) |
| Change timelock delay (`SetTimelockDelay`) | No | Yes | No | No | Timelock (current delay) |
| Pause / unpause (`pause` / `unpause`) | Yes | No | No | No | Admin direct (immediate) |
| Set transfer fee (`set_transfer_fee`) | Yes | No | No | No | Admin direct (immediate) |
| Set expiration duration (`set_expiration_duration`) | Yes | No | No | No | Admin direct (immediate) |
| Set fee params (`set_fee_params`) | Yes | No | No | No | Admin direct (immediate) |
| Set stake config (`set_stake_config`) | Yes | No | No | No | Admin direct (immediate) |
| Set bridge relayer (`set_bridge_relayer`) | Yes | No | No | No | Admin direct (immediate) |
| Set chain status (`set_chain_status`) | Yes | No | No | No | Admin direct (immediate) |
| Set name / symbol (`set_name` / `set_symbol`) | Yes | No | No | No | Admin direct (immediate) |
| Migrate (`migrate`) | Yes | No | No | No | Admin direct (immediate) |
| Backfill wrap periods (`backfill_wrap_periods`) | Yes | No | No | No | Admin direct (immediate) |
| Cancel proposed admin (`cancel_proposed_admin`) | Yes | No | No | No | Admin direct (immediate) |
| Bridge relay (mint / release) | No | No | No | Yes | Bridge relayer (immediate) |

### Fastest path is the real security property

The slowest route is not what protects the contract — the *fastest* route to
each action is. Two consequences follow directly from the table above:

- **Actions with an admin-direct route are only as safe as the admin key.**
  `pause`, fee/config setters, `set_bridge_relayer`, `set_chain_status`,
  `set_name`/`set_symbol`, `migrate`, and `backfill_wrap_periods` all take
  effect in the same transaction the admin signs. The timelock does not slow
  them down, because no `TimelockAction` variant exists for them.
- **Actions with only a timelock route are protected by the delay.** Key
  rotation, WASM upgrade, whitelist root changes, and delay changes cannot be
  performed faster than `TimelockDelay` (minimum 1 hour).

### Actions reachable by more than one route

Only one action is reachable through multiple routes with **different
guarantees**: changing the `Admin`.

- **Admin direct** — immediate, single transaction.
- **Timelock** — delayed by `TimelockDelay`, cancellable by the admin before
  execution.
- **Governance** — delayed by the voting period, and additionally by
  `TimelockDelay` when the timelock is enabled.

Because the admin-direct route is immediate, the effective guarantee for
changing the admin is the *weakest* of the three: a compromised admin key can
hand over ownership with no delay, regardless of the timelock or governance
configuration. The timelock and governance routes add observability but do not
raise the floor set by the direct route.

No other privileged action is reachable by more than one route.
