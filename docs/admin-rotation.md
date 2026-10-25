# Admin Rotation Procedure

This document describes the safe procedure for rotating admin control of the
Stellar Wrap Contract. Operators should follow it any time the admin address or
the Ed25519 signing pubkey needs to change.

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
monitor via `get_admin` (see [Verification](#verification)).

### Cancel a pending proposal

If a mistake is detected **after** `propose_admin` but **before** `accept_admin`,
the current admin can cancel:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <CURRENT_ADMIN_SECRET> \
  -- cancel_proposed_admin
```

Only one proposal may be open at a time. `propose_admin` will panic with
`AdminTransferProposalExists` if you try to open a second one without cancelling
the first.

---

## Verification

Run these queries after every rotation to confirm the expected state:

```bash
# 1. Confirm new admin is active
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- get_admin

# 2. Confirm no pending proposal remains
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- get_pending_admin

# 3. Confirm contract health (initialized, has_admin, has_signing_key)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  -- health

# 4. Smoke-test admin access with a no-op privileged call (e.g. unpause if paused, or pause+unpause)
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <NEW_ADMIN_SECRET> \
  -- is_paused
```

---

## Event monitoring

A successful `update_admin` call emits one event:

- **Topic 0:** `admin` (`Symbol`)
- **Topic 1:** `updated` (`Symbol`)
- **Data:** `(old_admin, new_admin)` — both as `Address`

Indexers can watch for this event to detect admin rotations without polling, but
must always verify the live admin via `get_admin` before enforcing privileged
flows. **Do not infer state from events alone.**

`accept_admin` does not currently emit an event; use `get_admin` to confirm.

---

## Signing pubkey rotation

The Ed25519 signing pubkey (`DataKey::AdminPubKey`) is set during `initialize`
and is **not** exposed via a dedicated rotation function.

### Pubkey validation rules

Both `initialize` and any future pubkey rotation reject obviously invalid
Ed25519 signing public keys before they are stored:

- The all-zero 32-byte key (`[0u8; 32]`) is rejected, because it is a
  degenerate value that cannot correspond to a legitimate signing key.
- Keys that are not exactly 32 bytes are rejected by construction.

Accepted keys are any other 32-byte value. Operators must still generate keys
with a proper Ed25519 keygen tool; validation only catches trivially bogus input.
Tests cover both rejected (all-zero) and accepted (non-zero) keys. Rotating it requires a
contract upgrade that includes a migration:

1. Deploy a new WASM version that exposes an `update_admin_pubkey` function (or
   equivalent migration path).
2. Call `upgrade` with the new WASM hash (requires admin auth):
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <ADMIN_SECRET> \
     -- upgrade \
     --new_wasm_hash <NEW_WASM_HASH_HEX>
   ```
3. Call `migrate` with the new migration version to apply the key change:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <ADMIN_SECRET> \
     -- migrate \
     --version <NEXT_MIGRATION_VERSION>
   ```
4. Verify with `health` that `has_signing_key` is still `true` and test a mint
   with a payload signed by the new key on testnet before mainnet.

> **Key difference from admin address rotation:** rotating the signing pubkey
> requires a contract upgrade and is a more involved process. Plan for a
> maintenance window and test thoroughly on testnet.

---

## Rollback plan

| Scenario | Recovery action |
|---|---|
| Wrong address proposed, not yet accepted (Ruta B) | Current admin calls `cancel_proposed_admin` |
| Tx not sent yet (Ruta A) | Do not broadcast; re-run with correct address |
| Wrong admin active, new admin **cooperates** | New admin calls `update_admin` to the correct address |
| Wrong admin active, new admin **does not cooperate** | Requires governance → timelock `update_admin`; subject to timelock delay |

**The safest default is always Ruta B (propose + accept).** It provides a
cancel window and requires the new admin to prove key control before the old
admin loses access.

For mainnet rotations, always rehearse the full procedure on testnet with the
exact addresses involved before executing on mainnet.

---

## Related documentation

- [README — Contract layout](../README.md#contract-layout) — where `admin.rs`
  sits in the overall module map.
- [Timelock](./timelock.md) — delayed execution that gates privileged admin
  operations.
- [Incident runbook](./incident-runbook.md) — what to do if admin control is
  lost or compromised.
