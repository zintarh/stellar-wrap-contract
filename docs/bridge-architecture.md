# Generic Token Bridge Interface Architecture

This document describes the design and implementation of the Generic Token Bridge Interface for cross-chain wrap interactions in `stellar-wrap-contract`.

## Overview

The Generic Token Bridge Interface allows `stellar-wrap-contract` to interact seamlessly with external blockchains (e.g., Ethereum, Polygon, Solana). It enables users to transfer/bridge wrap records off-chain to target chains and allows a threshold of appointed relayer keys to process inbound cross-chain wrap transfers onto Stellar.

The contract does not observe other chains. What a user is trusting is stated in [Trust model](#trust-model).

---

## Trust model

A user is trusting the admin, and trusting a threshold of relayer keys for anything that arrives from another chain. This section is enough to judge that trust without reading `bridge.rs`.

### Who the relayers are, and who appoints them

Relayers are off-chain operators. For each chain id they are an Ed25519 public key (`BytesN<32>`) in that chain's relayer set. The admin appoints the set and its threshold by calling `set_bridge_relayers(chain_id, relayers, threshold)`. Replacing the set or the threshold is the same call, and it requires the admin's authorization. No one else can appoint or remove a relayer.

`set_bridge_relayer` stores one Stellar `Address` separately. Inbound fulfillment and refunds do not authorize against that address.

### What a compromised relayer can do

`bridge_wrap_in` accepts a message once `threshold` distinct keys from that chain's set have signed it. Whoever can produce those signatures can:

- Mint a new active wrap on Stellar for any recipient who has not opted out, using any unused source nonce and any valid period, archetype, and data hash. The contract does not check that anything was locked on the source chain, so the mint can be unbacked.
- If that recipient already has a wrap for the same period, restore it from `Bridged` when the lifecycle rules allow the transition. That is the inbound path that unlocks a wrap this contract already holds.

A compromised relayer cannot do any of the following, even with a full threshold of signatures:

- Appoint or remove relayers, change the threshold, or enable or disable a chain. Those calls require the admin.
- Pause the contract, upgrade it, or rotate the admin.
- Transfer, burn, or revoke a wrap.
- Start an outbound bridge for a user. `bridge_wrap_out` requires that user's authorization.
- Refund an outbound request. `bridge_wrap_refund` requires the admin's authorization. Relayer keys are not sufficient.

A relayer who holds fewer keys than the threshold cannot complete an inbound message. They can refuse to sign. If the keys that are still willing to sign drop below the threshold, inbound fulfillment for that chain stops. That case is covered under [Threshold](#threshold).

### What the contract checks, and what it takes on someone's word

The contract checks the following itself:

- **Outbound.** The contract is not paused. The user authorized the call. The destination chain is enabled. The recipient payload is non-empty. The user's wrap exists and the lifecycle rules accept the move to `Bridged`.
- **Inbound.** The contract is not paused. The recipient has not opted out. The source chain is enabled. The period is a valid `YYYYMM`. The pair `(source_chain, source_nonce)` has not already been processed. At least `threshold` distinct public keys from that chain's set signed the inbound payload: this contract's address, the source chain, the source nonce, the recipient, the period, the archetype, and the data hash.
- **Refund.** The contract is not paused. An outbound request with that nonce is stored. A relayer set is stored for the request's destination chain. The wrap exists and can be restored from `Bridged`. The admin authorized the call.

The contract does not check the other chain. It accepts the following because a threshold of relayers signed it:

- That a source-chain transaction locked or burned a wrap.
- That `source_nonce` identifies a real event on that chain.
- That the recipient, period, archetype, and data hash are the values from that event.

A refund is not a relayer attestation. The contract accepts the admin's authorization as proof that the destination rejected the outbound transfer. It does not verify a rejection proof, and it does not verify relayer signatures on the refund. The implementation records that signature checks for refunds are not done yet and requires the admin instead.

`bridge_wrap_out` only stores the request and emits `br_out`. It does not submit the transfer to the destination chain and does not learn whether that chain minted anything. Delivery on the destination is entirely on the relayers, off this contract.

### Failure modes

**Stuck outbound request.** `bridge_wrap_out` moves the wrap to `Bridged` before any destination chain acts, and it stores an `OutboundBridgeRequest`. From then on the user cannot transfer, burn, revoke, bridge the same wrap again, or call `transition_wrap_state` to leave `Bridged`. The contract has no timeout and no user cancellation. If the relayers never complete the transfer on the destination and the admin never refunds the request, the wrap stays `Bridged`.

**Refund.** `bridge_wrap_refund(outbound_nonce)` is the unlock after a destination is said to have rejected the transfer. The admin must authorize it. The destination chain must already have a relayer set, or the call fails with `BridgeNotInitialized`. The chain does not have to still be enabled, and no relayer signature is required. The call restores the wrap through `restore_from_bridge` and emits `br_refund`. It leaves the outbound request in storage, and it does not prove the destination rejected the transfer. If the destination already minted and the admin still refunds, the user is credited on both sides and this contract cannot see that. If the admin does not refund, the user still cannot unlock the wrap. The only other exit from `Bridged` is an inbound message that restores that same wrap, and that message needs a threshold of relayer signatures.

**Disabled chain.** The admin disables a chain with `set_chain_status(chain_id, false)`. Chain id `0` is never enabled. `bridge_wrap_out` and `bridge_wrap_in` then fail with `ChainDisabled` and do not change wrap state. Disabling a chain does not restore wraps already moved to `Bridged` toward that chain, and it does not remove the relayer set. Those wraps remain stuck until the admin refunds them, or until an inbound message restores that same wrap.

### Threshold

Each chain stores its public keys and a `threshold` together. `set_bridge_relayers` rejects `threshold == 0` and rejects a threshold greater than the number of keys in that call (`InvalidThreshold`). A set that is already shorter than its threshold cannot be installed. A rejected call leaves the previous set in place.

`bridge_wrap_in` counts distinct keys. The same key listed twice, or signing twice, counts once. The call fails with `InvalidSignature` unless at least `threshold` distinct configured keys signed the payload.

The set falls below the threshold when fewer than `threshold` distinct keys can still produce a valid signature: keys are lost, relayers stop signing, or the configured list contains fewer distinct keys than `threshold`. Inbound fulfillment for that chain then stops. The contract does not lower the threshold, does not accept a smaller quorum, and does not let the user force the message through. Outbound locking on a chain that is still enabled continues to succeed, so new requests can become stuck while inbound is frozen. The admin restores service by appointing a new set whose distinct keys meet the threshold they set, and by refunding outbound requests that should be unlocked.

---

## Key Components & Workflow

### 1. Administration & Network Registry

- **Bridge relayers (`set_bridge_relayers` / `get_bridge_relayers`)**:
  - The admin appoints, per chain id, the Ed25519 public keys and the signature threshold. See [Trust model](#trust-model).
  - Inbound fulfillment checks that threshold of signatures. It does not use `require_auth()` on a relayer address.
- **Refund address (`set_bridge_relayer` / `get_bridge_relayer`)**:
  - The admin may store one Stellar address. Refunds and inbound fulfillment do not authorize against it. Refunds require the admin.

- **Supported Chain Registry (`set_chain_status` / `is_chain_supported`)**:
  - Chains are identified by unique numeric network IDs (e.g., `1` for Ethereum Mainnet, `137` for Polygon, `900` for Solana).
  - Outbound and inbound bridge operations verify that target/source chains are active before proceeding.

### 2. Outbound Cross-Chain Wrap (`bridge_wrap_out`)

1. **Initiation**: A user calls `bridge_wrap_out(user, destination_chain, recipient_address, period)`.
2. **Validation**:
   - Contract must not be paused.
   - User must authorize the transaction (`user.require_auth()`).
   - Destination chain ID must be enabled.
   - Recipient address payload must be non-empty.
3. **State Transition**:
   - The user's local wrap record transitions from `Active` to terminal `Bridged` using the Wrap Lifecycle FSM.
   - `Bridged` records cannot be transferred, burned, re-bridged, or reactivated by the user.
4. **Nonce & Storage**:
   - Monotonically increasing `OutboundBridgeNonce` counter is incremented.
   - An `OutboundBridgeRequest` record is written to persistent storage.
5. **Event Emission**: Emits `br_out` event containing user, destination chain, nonce, recipient address, and wrap period.

### 3a. Outbound Refund

- If the destination chain rejects an outbound request, the admin calls
   `bridge_wrap_refund(outbound_nonce)`. Relayer signatures are not checked.
- The request must identify an existing `Bridged` record; the call restores
   it to `Active` and the contract emits `br_refund`. A relayer set must
   already be stored for the destination chain.
- The public `transition_wrap_state` entry point cannot exit `Bridged`, so only
   this admin-authorized settlement path, or an inbound restore of that same
   wrap, can unlock it. There is no user-initiated unlock. See
   [Failure modes](#failure-modes).

### 3. Inbound Cross-Chain Wrap (`bridge_wrap_in`)

1. **Relayer Execution**: A caller submits `bridge_wrap_in(source_chain, source_nonce, recipient, period, archetype, data_hash, signatures)`.
2. **Validation & Replay Protection**:
   - At least `threshold` distinct keys from the source chain's relayer set must have signed the inbound payload. Fewer valid signatures fail with `InvalidSignature`.
   - Source chain must be active. A disabled chain fails with `ChainDisabled`.
   - `InboundBridgeProcessed(source_chain, source_nonce)` ensures each cross-chain transaction can only be processed once (preventing double-spend / replay attacks).
3. **Wrap Minting / Activation**:
   - Validates period structure (`YYYYMM` format, between `MIN_PERIOD_YEAR = 2024` and `MAX_PERIOD_YEAR = 2100` with months `01..=12`, enforced by shared `validate_period`).
   - If wrap record does not exist on Stellar, creates a new active wrap record for `recipient` and updates wrap counts and latest period metadata.
    - If wrap record already exists, transitions state to `Active` through the
       FSM; illegal transitions fail with `InvalidStateTransition`.
4. **Record & Event**:
   - Stores `InboundBridgeRecord(source_chain, source_nonce)` in persistent storage.
   - Emits `br_in` event with recipient address, source chain, source nonce, and period.

---

## Data Structures & Storage Keys

### Data Types

```rust
pub struct OutboundBridgeRequest {
    pub nonce: u64,
    pub sender: Address,
    pub destination_chain: u32,
    pub recipient_address: Bytes,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub timestamp: u64,
}

pub struct InboundBridgeRecord {
    pub source_chain: u32,
    pub source_nonce: u64,
    pub recipient: Address,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub timestamp: u64,
}
```

### Storage Keys (`DataKey`)

- `BridgeRelayer`: Stellar `Address` stored by `set_bridge_relayer`. Not used to authorize inbound messages or refunds.
- `BridgeRelayerSet(u32)`: Per chain id, the relayer public keys and the `threshold`.
- `BridgeChainStatus(u32)`: Status flag (`bool`) per chain ID.
- `OutboundBridgeNonce`: Monotonic counter (`u64`).
- `OutboundBridgeRequest(u64)`: Outbound request keyed by nonce.
- `InboundBridgeProcessed(u32, u64)`: Replay flag keyed by `(source_chain, source_nonce)`.
- `InboundBridgeRecord(u32, u64)`: Inbound record keyed by `(source_chain, source_nonce)`.

---

## Security Audit & Guarantees

1. **Replay Protection**: Inbound nonces are recorded per source chain in persistent storage to prevent replay attacks.
2. **Access Control**: The admin appoints relayers, sets the threshold, and enables or disables chains (`set_bridge_relayers`, `set_bridge_relayer`, `set_chain_status`). Inbound wraps require a threshold of relayer signatures, not a relayer `require_auth()`. Refunds require the admin. See [Trust model](#trust-model).
3. **Emergency Pause**: Main contract pause flag immediately halts both outbound and inbound bridge operations.
4. **Storage TTL Management**: Persistent entries (outbound requests, inbound records, processed flags) have TTL set to 1 year (~17,280 * 365 ledgers).

---

## Decision Records

The bridge contract's security posture is governed by two architecture decision
records. They are filed under `docs/` alongside this document and are linked
here so a reader finds them in context:

- [`docs/PROXY_PATTERN_DECISION.md`](./PROXY_PATTERN_DECISION.md) — the proxy
  pattern decision. It records that the bridge does **not** use a batching
  proxy contract (issue #517); each bridge entry point is called directly and
  authorization is enforced per call. This decision is enforced by the
  `proxy_pattern_decision` test module, which fails if a batching proxy entry
  point is introduced without updating the record.
- [`docs/SIGNATURE_VERIFICATION_DECISION.md`](./SIGNATURE_VERIFICATION_DECISION.md)
  — the signature verification decision record. Inbound fulfillment as
  implemented checks a threshold of Ed25519 signatures from the chain's
  relayer set. The trust assumptions of that check are in
  [Trust model](#trust-model), not in a relayer `require_auth()`.

Both records carry a review checklist item (see the "Enforcement" section of
each record) so a reviewer can catch a violation during code review even
before the executable test runs.

---

## Bridge Architecture and Authority Model

This section describes how cross-chain messages are relayed into the contract and how the bridge relayer fits into the contract's overall authority model.

### Relayers

Bridge relayers are Ed25519 keys appointed per chain by the admin, together with a signature threshold. They can attest inbound messages. They cannot pause, upgrade, rotate the admin, change the relayer set, or refund. Who they are, who appoints them, and what a compromised key can do are specified in [Trust model](#trust-model). The admin appointment path is the same admin authority described in `docs/admin-rotation.md`.

### Authority Model

Privileged actions in this contract are reachable through more than one route. The routes are:

1. **Admin direct** — the admin calls the privileged function directly. No delay. The admin can cancel any pending governance or timelock action.
2. **Governance proposal** — token holders propose and vote; on success the action is queued. Delay is the governance voting period plus the timelock delay. The admin or governance can cancel a queued proposal before execution.
3. **Timelock** — a queued action executes after the timelock delay elapses. The admin can cancel a queued action before it executes. See `docs/timelock.md`.
4. **Bridge relayer** — a threshold of registered relayer keys signs an inbound `bridge_wrap_in` message. No delay beyond collecting those signatures. The admin can replace the relayer set, which rejects later messages under the old keys. A message already accepted and applied is not reversed by removing a relayer. Relayer signatures do not authorize pause, unpause, upgrade, withdrawal, relayer-set changes, or refunds.

### Fastest path per action

The fastest path to any privileged action is the one with the smallest delay, not the largest. For most actions the admin direct route is fastest (no delay). `bridge_wrap_in` is the action the admin cannot perform: it waits only on a threshold of relayer signatures. The governance and timelock routes are always slower because they include a voting period and/or a timelock delay.

### Actions reachable by multiple routes

Any action reachable by both the admin direct route and the governance/timelock route has different guarantees depending on the route: the admin route is immediate and cancellable only by the admin, while the governance route is delayed and cancellable by the admin or governance. Inbound bridge fulfillment is reachable only by a threshold of relayer signatures. The admin can replace the relayer set but cannot reverse a `bridge_wrap_in` that has already been applied.

### Privileged Actions

Each privileged function in the contract appears exactly once below, with all of its routes.

| Privileged action | Admin direct | Governance proposal | Timelock | Bridge relayer |
| --- | --- | --- | --- | --- |
| `setAdmin` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `setRelayer` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `setTimelockDelay` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `pause` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `unpause` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `upgrade` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `withdraw` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `bridge_wrap_in` | no | no | no | yes (threshold of relayer signatures) |
| `bridge_wrap_refund` | yes (no delay) | no | no | no |

### Who may initiate, delay, and cancel

- **Admin direct:** initiated by the admin; no delay; cancellable only by the admin (by not calling it).
- **Governance proposal:** initiated by any token holder meeting the proposal threshold; delay is the voting period plus the timelock delay; cancellable by the admin or by governance before execution.
- **Timelock:** initiated by the admin or by a passed governance proposal; delay is the timelock delay; cancellable by the admin before execution.
- **Bridge relayer:** inbound fulfillment is initiated by submitting a threshold of signatures from the admin-appointed set; delay is only the time to collect those signatures; the admin can replace the set so later messages fail, and cannot undo a message already applied. Refunds are not in this route.

### Cross-references

- `docs/admin-rotation.md` covers the admin direct route and admin rotation.
- `docs/timelock.md` covers the timelock route and timelock delay.

---

## Related Documentation

- [README contract layout](../README.md#contract-layout) — full module map for `src/`.
- [Admin rotation](admin-rotation.md) — `admin.rs` and `governance.rs`.
- [Timelock](timelock.md) — `timelock.rs`.
- [Revoke policy](revoke-policy.md) — `revoke.rs`.
- [Whitelist merkle](whitelist-merkle.md) — `merkle.rs`.
- [Signing payload](signing-payload.md) — `signature.rs`.
- [Verify data](verify-data.md) — `queries.rs`.
- [Incident runbook](incident-runbook.md) — operational procedures.
