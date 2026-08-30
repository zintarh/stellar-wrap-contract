# Generic Token Bridge Interface Architecture

This document describes the design and implementation of the Generic Token Bridge Interface for cross-chain wrap interactions in `stellar-wrap-contract`.

## Overview

The Generic Token Bridge Interface allows `stellar-wrap-contract` to interact seamlessly with external blockchains (e.g., Ethereum, Polygon, Solana). It enables users to transfer/bridge wrap records off-chain to target chains and allows authorized bridge relayers to process inbound cross-chain wrap transfers onto Stellar.

---

## Key Components & Workflow

### 1. Administration & Network Registry

- **Bridge Relayer (`set_bridge_relayer` / `get_bridge_relayer`)**:
  - The admin configures an authorized relayer or bridge validator contract.
  - Inbound bridge operations require explicit authorization (`relayer.require_auth()`).

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

- If the destination chain rejects an outbound request, the configured bridge
   relayer calls `bridge_wrap_refund(outbound_nonce)`.
- The request must identify an existing `Bridged` record; the relayer restores
   it to `Active` and the contract emits `br_refund`.
- The public `transition_wrap_state` entry point cannot exit `Bridged`, so only
   this relayer-authorized settlement path can unlock the wrap.

### 3. Inbound Cross-Chain Wrap (`bridge_wrap_in`)

1. **Relayer Execution**: An authorized bridge relayer calls `bridge_wrap_in(source_chain, source_nonce, recipient, period, archetype, data_hash)`.
2. **Validation & Replay Protection**:
   - Relayer authorization is verified (`relayer.require_auth()`).
   - Source chain must be active.
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

- `BridgeRelayer`: Configured relayer `Address`.
- `BridgeChainStatus(u32)`: Status flag (`bool`) per chain ID.
- `OutboundBridgeNonce`: Monotonic counter (`u64`).
- `OutboundBridgeRequest(u64)`: Outbound request keyed by nonce.
- `InboundBridgeProcessed(u32, u64)`: Replay flag keyed by `(source_chain, source_nonce)`.
- `InboundBridgeRecord(u32, u64)`: Inbound record keyed by `(source_chain, source_nonce)`.

---

## Security Audit & Guarantees

1. **Replay Protection**: Inbound nonces are recorded per source chain in persistent storage to prevent replay attacks.
2. **Access Control**: Admin authorization is enforced for configuration (`set_bridge_relayer`, `set_chain_status`), and Relayer authorization is enforced for inbound wraps.
3. **Emergency Pause**: Main contract pause flag immediately halts both outbound and inbound bridge operations.
4. **Storage TTL Management**: Persistent entries (outbound requests, inbound records, processed flags) have TTL set to 1 year (~17,280 * 365 ledgers).
