# Admin Rotation Procedure

This document describes the safe procedure for rotating admin control of the
Stellar Wrap Contract. Operators should follow it any time the admin address or
the Ed25519 signing pubkey needs to change.

---

## Signing pubkey rotation: planned vs emergency

`update_admin_pubkey` (reached via `upgrade` + `migrate`) swaps the stored
verification key (`DataKey::AdminPubKey`) **immediately**. There is currently
**no grace window**: the old key is not retained, and no `(old_key, expires_at)`
overlap is supported. This is the current, intended behaviour and is covered by
tests in `src/admin.rs`.

### What happens to in-flight signatures

A mint payload is verified against the *currently stored* pubkey at the moment
the mint is processed. Therefore:

- A signature produced under the **old** key with a future `valid_until`
  **fails immediately** after rotation — the old key is no longer trusted.
- A signature produced under the **new** key **succeeds** after rotation.

Both cases are asserted by tests (`rotation_invalidates_old_key_signatures` and
`rotation_accepts_new_key_signatures`).

### Emergency rotation (compromise)

Immediate invalidation is exactly what you want here: rotate the pubkey as fast
as possible so any signatures forged with the compromised key stop verifying at
once. Expect in-flight mints signed with the old key to fail; that is the point.

1. Rotate the pubkey via `update_admin_pubkey` (admin direct path, no delay).
2. Optionally `pause` mints first if you need to drain the relayer queue.
3. Re-issue any legitimate in-flight payloads under the new key.

### Planned rotation

Because there is no grace window, a planned rotation will cause any payload
signed under the old key but not yet submitted to fail. To avoid failed user
mints during a planned rotation:

1. **Drain first:** stop issuing new old-key payloads and let the relayer submit
   everything already signed before rotating.
2. **Rotate:** call `update_admin_pubkey` with the new key.
3. **Switch signers:** begin signing new payloads with the new key immediately
   after rotation.

If a zero-downtime overlap is required, that needs the optional grace window
`(old_key, expires_at)` described in issue #1002; until it exists, treat
rotation as a hard cutover and drain in-flight signatures beforehand.

---

## Authority model: who can change what, and how fast

Privileged action is reachable through four routes: the **admin directly**, a
**governance proposal**, the **timelock**, and **bridge relayers**. This section
enumerates every privileged action and every route that reaches it, states who
may initiate, what delay applies, and who can cancel, and identifies the
**fastest** path to each action — the fastest path, not the slowest, is the
contract's real security property.

### Routes at a glance

| Route | Who may initiate | Delay | Who can cancel |
|---|---|---|---|
| Admin direct | Current admin (`DataKey::Admin`) | None — immediate | Nobody (irreversible in-tx) |
| Governance proposal | Governance proposer | None to *schedule*; execution still waits on the timelock | Governance (cancel the proposal) |
| Timelock | Admin or governance scheduler | Configured timelock delay (cannot be shortened below the minimum) | Admin or the scheduler that queued it |
| Bridge relayer | Registered relayer | None — immediate on valid relay | Admin (pause / rotate relayer) |

### Privileged actions and their routes

Each privileged function appears exactly once, with all of its routes.

| Privileged action | Routes that reach it | Fastest path |
|---|---|---|
| `update_admin` | Admin direct; governance → timelock | Admin direct (no delay) |
| `propose_admin` | Admin direct | Admin direct (no delay) |
| `accept_admin` | Pending admin direct | Pending admin direct (no delay) |
| `cancel_proposed_admin` | Admin direct | Admin direct (no delay) |
| `pause` | Admin direct; governance → timelock | Admin direct (no delay) |
| `unpause` | Admin direct; governance → timelock | Admin direct (no delay) |
| `upgrade` | Admin direct; governance → timelock | Admin direct (no delay) |
| `migrate` | Admin direct; governance → timelock | Admin direct (no delay) |
| `update_admin_pubkey` (via upgrade + migrate) | Admin direct; governance → timelock | Admin direct (no delay) |
| Mint (payload-signed) | Bridge relayer | Bridge relayer (no delay) |

### Actions reachable by more than one route with different guarantees

- **`update_admin`, `pause`, `unpause`, `upgrade`, `migrate`** are each reachable
  both by the admin directly (immediate, uncancellable) and via governance →
  timelock (delayed, cancellable). The two routes carry **different guarantees**:
  the direct route is instant and irreversible, while the governance route is
  subject to the timelock delay and can be cancelled before execution. The
  fastest path is always the direct admin route, so the timelock does **not**
  slow down a compromised or malicious admin.
- **Mint** is reachable only through the bridge relayer path and is immediate on
  a valid relay; it is not gated by the timelock. The admin's only lever over it
  is `pause` (immediate) or rotating the relayer.

### Fastest-path summary

For every privileged action except mint, the fastest path is the **direct admin
call with no delay**. The timelock and governance routes add delay and
cancellability but never make an action *faster*. Mint's fastest (and only) path
is the **bridge relayer**, also with no delay. Treat the admin key and the
relayer key as the true security boundary; the timelock is a governance
convenience, not a speed limit on the admin.

---

## Governance and timelock interaction

The contract exposes two overlapping paths to privileged action: the governance
module and the timelock module. Their relationship is explicit and is covered by
tests in `test_governance_timelock_interaction`.

### Does a governance proposal execute immediately?

**No.** A governance proposal never executes a privileged action directly. It
only *schedules* the action into the timelock. The action becomes executable
only after the timelock delay has elapsed and the timelock entry is executed.
This is enforced in code: the governance execution path routes through the
timelock scheduler rather than calling the privileged function inline.

### Can a timelocked action change the admin while a proposal is open?

**No.** A timelocked admin change and an open governance proposal to change the
admin cannot both take effect. The admin-change path is guarded so that a
timelocked action cannot silently overwrite an admin while a governance proposal
to change the admin is pending. Tests assert that the second attempt is rejected
and the pending proposal is preserved.

### Can governance schedule, cancel, or shorten a timelocked action?

- **Schedule:** yes — this is the only way governance reaches a privileged
action.
- **Cancel:** yes — governance can cancel a timelocked action it scheduled.
- **Shorten:** no — governance cannot reduce the timelock delay below the
  configured minimum. Tests assert that an attempt to shorten the delay is
  rejected.

### Is there a path that bypasses both mechanisms?

**No.** Every privileged action is reachable only through either the direct
admin path or the timelock path, and the governance path itself funnels through
the timelock. Tests confirm there is no call sequence that reaches a privileged
action while bypassing both the admin authorization and the timelock delay.

---

## Two independent rotation types

The contract stores two separate privileged values. Rotating one does **not**
affect the other.

| Value | Storage key | Controls |
|---|---|---|
| Admin address | `DataKey::Admin` | Authorization for all admin-only functions (`update_admin`, `pause`, `upgrade`, `migrate`, …) |
| Signing pubkey | `DataKey::AdminPubKey` | Ed25519 public key used to verify mint payload signatures |

---

## Admin address rotation

### Preparation

1. Confirm the **current** admin on-chain before starting:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     -- get_admin
   ```
2. Have the new admin verify they control their private key by signing a test
   transaction on testnet first.
3. Ensure no pending proposal already exists:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     -- get_pending_admin
   ```
   If a proposal exists, cancel it before proceeding (see
   [Cancel a pending proposal](#cancel-a-pending-proposal)).

### Ruta A — Direct transfer (`update_admin`)

> **Warning:** This path is immediate and irreversible in the same transaction.
> Only use it when the new admin address has already been verified on testnet.

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <CURRENT_ADMIN_SECRET> \
  -- update_admin \
  --new_admin <NEW_ADMIN_ADDRESS>
```

The contract replaces the stored admin address and emits an
`("admin", "updated")` event immediately.

### Ruta B — Two-step transfer (`propose_admin` + `accept_admin`) — recommended

This path keeps the current admin in control until the new admin explicitly
accepts, protecting against address typos and key-loss scenarios.

**Step 1 — Current admin proposes a new admin:**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <CURRENT_ADMIN_SECRET> \
  -- propose_admin \
  --new_admin <NEW_ADMIN_ADDRESS>
```

**Step 2 — Verify the pending admin is correct:**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- get_pending_admin
```

Confirm the returned address matches the intended new admin before proceeding.

**Step 3 — New admin accepts:**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <NEW_ADMIN_SECRET> \
  -- accept_admin
```

The contract atomically moves `PendingAdmin` → `Admin` and clears the pending
slot. An `("admin", "updated")` event is **not** emitted by `accept_admin`;
monitor via `get_admin` (see [Verificat
