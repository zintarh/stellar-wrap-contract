# Incident Runbook: Compromised Admin Key

This runbook covers the **unplanned** case where an admin key is suspected to be compromised and an attacker may already be acting. It is distinct from the planned rotation procedure in [admin-rotation.md](admin-rotation.md).

> **Goal:** contain the blast radius as fast as possible, then recover.

---

## 1. Immediate containment — pause the contract

### Who can do it

Anyone who can sign a transaction with the **current admin address** can call `pause`. The contract does not gate `pause` behind any additional role — it only requires `admin.require_auth()`.

### How fast

`pause` is a single on-chain transaction. Once it is confirmed, every entrypoint that calls `require_not_paused` is blocked for all users, including the attacker.

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network <NETWORK> \
  --source <ADMIN_SECRET> \
  -- pause
```

### What pause stops

| Entrypoint | Effect while paused |
|---|---|
| `mint_wrap` / `mint_wrap_batch` | Blocked — no new wraps can be created |
| `bridge_wrap_out` | Blocked — no outbound bridges |
| `bridge_wrap_in` | Blocked — no inbound bridges |
| `bridge_wrap_refund` | Blocked — no refunds |
| `stake` / `unstake` / `withdraw_stake` | Blocked — staking paused |
| `transition_wrap_state` | Blocked — state changes paused |
| `expire_wrap` | Blocked — no forced expiry |

See [pause_coverage_test.rs](https://github.com/zintarh/stellar-wrap-contract/blob/main/src/pause_coverage_test.rs) for the full list of blocked and allowed entrypoints.

### What pause does **not** stop

The following entrypoints remain callable while paused because they are needed for recovery and governance:

| Entrypoint | Why it stays active |
|---|---|
| `update_admin` | Admin rotation must always be possible |
| `propose_admin` / `accept_admin` / `cancel_proposed_admin` | Two-step handover must work |
| `upgrade` / `migrate` | Contract maintenance must be possible |
| `timelock_schedule` / `timelock_execute` / `timelock_cancel` | Timelock operations must be manageable |
| `set_bridge_relayer` / `set_bridge_relayers` / `set_chain_status` | Bridge configuration must be fixable |
| `set_whitelist_root` / `clear_whitelist_root` | Whitelist must be recoverable |
| `set_fee_params` / `set_transfer_fee` / `clear_transfer_fee` | Fee config must be recoverable |
| `revoke_wrap` | Admin revocation for compliance |
| `burn_wrap` | User-initiated irreversible burn |
| `transfer_wrap` | User-initiated wrap transfer |
| `set_name` / `set_symbol` | Metadata |
| `set_alias_hash` / `opt_out` / `opt_in` | User preferences |
| `extend_ttl` / `renew_all_ttls` | Storage preservation |
| `set_expiration_duration` | Config |
| `set_stake_config` | Staking config |
| `create_admin_proposal` / `vote_admin_proposal` / `execute_admin_proposal` / `cancel_admin_proposal` | DAO governance |

**This is by design.** Pausing is a traffic blocker, not a lockout. The attacker can still use any admin-level call that is not gated by `require_not_paused`.

---

## 2. What a compromised admin can still do (and why pausing alone is not enough)

### 2.1 Direct admin actions (no timelock)

If the timelock is **not enabled**, the compromised admin can immediately:

1. **Rotate the admin** via `update_admin` — hand over control to an address they own.
2. **Rotate the signing pubkey** via `update_admin_pubkey` — break the existing signature verification so the legitimate admin can no longer mint.
3. **Upgrade the WASM** via `upgrade` — deploy malicious code.
4. **Change the whitelist root** via `set_whitelist_root` — open or close allowlists.
5. **Change bridge relayers** via `set_bridge_relayers` / `set_bridge_relayer` — redirect inbound bridge flows.
6. **Change fee params** via `set_fee_params` / `set_transfer_fee` — extract value.

### 2.2 Timelocked actions (if timelock is enabled)

If the timelock **is enabled**, direct `update_admin` and `upgrade` calls are blocked (`TimelockRequired`). However, the compromised admin can still:

1. **Schedule a malicious admin handover** via `timelock_schedule(SetAdmin(attacker_address))`. The action will execute after the configured delay (minimum 1 hour).
2. **Schedule a WASM upgrade** via `timelock_schedule(Upgrade(malicious_wasm_hash))`. Same delay.
3. **Schedule a whitelist root change** via `timelock_schedule(SetWhitelistRoot(...))`.
4. **Schedule a bridge relayer change** via `timelock_schedule(SetBridgeRelayers(...))`.
5. **Schedule a timelock delay change** via `timelock_schedule(SetTimelockDelay(shorter_delay))` — shortening the delay speeds up all the above.
6. **Cancel existing legitimate scheduled operations** via `timelock_cancel(id)` — removing safety nets.

The timelock gives observers a window to react, but **it does not prevent a compromised admin from queuing malicious actions**. It only makes them delayed and visible.

### 2.3 Governance actions (if timelock is enabled)

When the timelock is enabled, `execute_admin_proposal` queues `TimelockAction::SetAdmin` instead of applying immediately. A compromised admin can:

1. **Create a proposal** via `create_admin_proposal` pointing to their own address.
2. **Vote on their own proposal** (if they hold governance voting power).
3. **Execute the proposal** once the voting period ends — this queues a `SetAdmin` action that will fire after the timelock delay.

### 2.4 Bridge relayer compromise (separate blast radius)

The bridge relayer is a **separate** authorization path from the admin. If the bridge relayer key is compromised:

- **Inbound bridge (`bridge_wrap_in`)**: The attacker can mint wraps on Stellar for any recipient, using any source chain and nonce, as long as they can produce valid signatures from the configured relayer set. This is a **minting attack** — the attacker creates wraps out of thin air.
- **Outbound bridge (`bridge_wrap_out`)**: Requires user authorization, so the attacker cannot initiate outbound bridges on behalf of users.
- **Bridge refund (`bridge_wrap_refund`)**: The attacker can restore bridged wraps to `Active` state, potentially allowing double-spend or re-bridging.

**Pausing the contract blocks all bridge entrypoints** (`bridge_wrap_out`, `bridge_wrap_in`, `bridge_wrap_refund`). However, if the attacker has already scheduled a bridge relayer change through the timelock, the new relayer will be active after execution and the attacker can resume.

The bridge relayer compromise has a **different blast radius** from an admin compromise:

| | Admin compromised | Bridge relayer compromised |
|---|---|---|
| Can mint new wraps | Yes (via `mint_wrap` with stolen signing key) | Yes (via `bridge_wrap_in` with stolen relayer key) |
| Can steal existing wraps | Yes (via `transfer_wrap` if admin can auth, or by rotating admin then transferring) | No (outbound requires user auth) |
| Can change contract code | Yes (via `upgrade`) | No |
| Can change admin | Yes | No |
| Can change bridge config | Yes | Yes (via `set_bridge_relayers`) |
| Can drain fees | Yes (via `set_transfer_fee` redirect) | No |

---

## 3. Recovery — rotating to a new admin

### 3.1 If you caught the compromise before the attacker acted

1. **Pause the contract** (see §1).
2. **Rotate the admin** immediately using the two-step handover (Ruta B from [admin-rotation.md](admin-rotation.md)):
   ```bash
   # Step 1: Propose a new admin (current admin, which you still control)
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <CURRENT_ADMIN_SECRET> \
     -- propose_admin \
     --new_admin <NEW_ADMIN_ADDRESS>
   ```
3. **Accept on the new admin key**:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <NEW_ADMIN_SECRET> \
     -- accept_admin
   ```
4. **Rotate the signing pubkey** if you suspect the old key was also compromised. This requires a contract upgrade (see [admin-rotation.md §Signing pubkey rotation](admin-rotation.md#signing-pubkey-rotation)).
5. **Unpause** after verification:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <NEW_ADMIN_SECRET> \
     -- unpause
   ```
6. **Verify** with `health` and a smoke test.

### 3.2 If the attacker rotated first

If the attacker has already called `update_admin` and the admin is now under their control:

1. **Pause the contract** immediately — this blocks further damage even though the attacker is admin.
2. **Check for pending timelocked operations**:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     -- timelock_pending
   ```
   If there are queued malicious operations (e.g., `SetAdmin` to the attacker, `Upgrade` to malicious WASM), **cancel them** if possible:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <CURRENT_ADMIN_SECRET> \
     -- timelock_cancel \
     --id <OPERATION_ID>
   ```
   Note: `timelock_cancel` requires the **current admin** auth. If the attacker is admin, you cannot cancel their queued operations.
3. **If you still have the old signing key** (the attacker only rotated the admin, not the pubkey): You can still mint wraps. Use this to create a "rescue" wrap that proves you have on-chain presence, then proceed to rotate the admin back via governance or by finding another path.
4. **If the attacker rotated both admin and pubkey**: The contract is fully in attacker control. Recovery requires:
   - **Governance intervention**: If the timelock is enabled and there is a governance proposal system, the community may be able to execute a proposal to reclaim admin (but this requires the voting period to end and the attacker not controlling the vote).
   - **Contract upgrade**: If a trusted upgrade path exists (e.g., a multisig or DAO-controlled upgrade mechanism), deploy a new WASM that resets admin control.
   - **Emergency fork / migration**: As a last resort, migrate users to a new contract instance.
5. **If the attacker changed bridge relayers**: Pause the contract to block bridge operations, then rotate the relayer back once you regain admin control.

### 3.3 If the attacker scheduled a timelocked action

1. **Pause the contract** to block entrypoints that would cause damage while waiting.
2. **Monitor the timelock queue**:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     -- timelock_pending
   ```
3. **If you still have admin auth**, cancel the malicious operation before its ETA:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <CURRENT_ADMIN_SECRET> \
     -- timelock_cancel \
     --id <OPERATION_ID>
   ```
4. **If you do not have admin auth** (attacker already rotated admin): You cannot cancel the operation. Wait for it to execute, then attempt recovery from the new state.

---

## 4. Bridge relayer compromise — specific response

### 4.1 Immediate action

1. **Pause the contract** — this blocks `bridge_wrap_in`, `bridge_wrap_out`, and `bridge_wrap_refund` immediately.
2. **Rotate the bridge relayer** to a new, secure address:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --network <NETWORK> \
     --source <ADMIN_SECRET> \
     -- set_bridge_relayers \
     --chain_id <CHAIN_ID> \
     --relayers <NEW_RELAYER_PUBKEY> \
     --threshold <THRESHOLD>
   ```
3. **Check for already-processed inbound nonces** — if the attacker already processed inbound bridge messages, those wraps are already minted. You cannot un-mint them; they must be revoked or burned individually.
4. **Review outbound bridge requests** — check if the attacker initiated any outbound bridges that are still pending. If so, the relayer refund path may be compromised.

### 4.2 Blast radius comparison

| Attack vector | Admin compromised | Bridge relayer compromised |
|---|---|---|
| Mint new wraps | Yes (signing key) | Yes (inbound bridge) |
| Mint wraps for specific user | Yes | Yes (any recipient) |
| Steal existing wraps | Yes (via transfer) | No |
| Change contract code | Yes | No |
| Change admin | Yes | No |
| Redirect bridge refunds | Yes (via bridge config) | Yes (relayer is refund auth) |
| Pause contract | Yes | No (pause is admin-only) |

**Key difference:** a bridge relayer compromise gives the attacker minting power but **not** contract control. The admin can still pause, rotate relayers, and revoke. An admin compromise gives full control including the ability to prevent the admin from pausing or rotating.

---

## 5. Post-incident checklist

After containing the incident and rotating credentials:

- [ ] Contract is unpaused and functioning normally.
- [ ] New admin address is confirmed via `get_admin`.
- [ ] No pending timelocked operations remain (or all are legitimate).
- [ ] Signing pubkey is rotated if the old key was compromised.
- [ ] Bridge relayers are set to trusted addresses.
- [ ] Whitelist root is set to the correct value.
- [ ] Transfer fee params are correct (not redirected).
- [ ] All user wrap records are intact (no unauthorized burns or revokes).
- [ ] Governance proposals are reviewed for any malicious proposals.
- [ ] Monitoring alerts are re-enabled and tested.
- [ ] Incident timeline and actions taken are documented for audit trail.

---

## 6. Speed reference

| Action | Time to execute | Who can do it |
|---|---|---|
| `pause` | ~5 seconds (one tx) | Current admin |
| `unpause` | ~5 seconds (one tx) | Current admin |
| `propose_admin` | ~5 seconds (one tx) | Current admin |
| `accept_admin` | ~5 seconds (one tx) | New admin |
| `cancel_proposed_admin` | ~5 seconds (one tx) | Current admin |
| `timelock_cancel` | ~5 seconds (one tx) | Current admin |
| `set_bridge_relayers` | ~5 seconds (one tx) | Current admin |
| `update_admin_pubkey` | ~5 seconds (one tx) | Current admin |
| Full admin rotation (Ruta B) | ~10 seconds + acceptance | Current admin + new admin |

**The fastest containment action is always `pause`.** Execute it first, then investigate and rotate.
