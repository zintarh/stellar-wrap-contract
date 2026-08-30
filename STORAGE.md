# Stellar Wrap Contract Storage Architecture & TTL Policy

This document defines the storage architecture, storage tier assignments, time-to-live (TTL) extension policies, size bounds, and fee accounting mechanics for all storage keys used in the `StellarWrapContract`.

---

## 1. Core Storage Rules & Guarantees

### Overarching Storage Tier Rule
> **Instance storage is for bounded configuration only.**
>
> Unbounded collections, user-generated data, per-operation records, and growing datasets must **never** be stored in instance storage. Unbounded instance entries increase the base footprint for every contract invocation, bloat transaction resource footprints, and risk exceeding ledger write limits. Any unbounded or per-user data must be placed in **Persistent storage** with active TTL management. Ephemeral locks and reentrancy guards belong in **Temporary storage**.

### Storage Tiers Overview
Soroban provides three storage tiers with distinct persistence, TTL, and rent properties:

1. **Instance Storage (`e.storage().instance()`)**:
   - Shared contract-level configuration and global protocol parameters.
   - Tied directly to the contract instance's lifetime; renewed whenever the instance TTL is extended.
   - Bounded in size to keep transaction fees and instance loading overhead minimal.

2. **Persistent Storage (`e.storage().persistent()`)**:
   - User data, wrap records, governance proposals, staking records, and cross-chain bridge records.
   - Individual entries require explicit TTL extension (`extend_ttl`) to remain live on-chain.
   - Baseline renewal policy: Extended to **~1 year** (`17,280 * 365 = 6,307,200` ledgers) on creation and modification.

3. **Temporary Storage (`e.storage().temporary()`)**:
   - Ephemeral reentrancy guards (`MintGuard`, `TransferGuard`).
   - Cleared automatically upon transaction completion or TTL expiry, preventing persistent state bloat.

---

## 2. Complete DataKey Variant Catalog

Every variant of the `DataKey` enum is documented below with its assigned storage tier, value type, TTL policy, size bounds, and architectural rationale.

| DataKey Variant | Storage Tier | Value Type | TTL Policy | Size Bound | Rationale |
|---|---|---|---|---|---|
| `Admin` | Instance | `Address` | Contract Instance Lifetime | 32 bytes | Core protocol admin authorization address. Fixed size configuration. |
| `AdminPubKey` | Instance | `BytesN<32>` | Contract Instance Lifetime | 32 bytes | Backend Ed25519 public key used for payload signature verification. |
| `PendingAdmin` | Instance | `Address` | Contract Instance Lifetime | 32 bytes | Proposed new admin address during two-step admin transfer handover. |
| `Wrap(Address, u64)` | Persistent | `WrapRecord` | 1 Year (`6,307,200` ledgers) | ~64–100 bytes | Core user wrap record keyed by `(user, period)`. Unbounded collection. |
| `WrapCount(Address)` | Persistent | `u32` | 1 Year (`6,307,200` ledgers) | 4 bytes | Number of active wraps owned by a user. |
| `LatestPeriod(Address)` | Persistent | `u64` | 1 Year (`6,307,200` ledgers) | 8 bytes | Highest period index minted for a user. |
| `WrapPeriods(Address)` | Persistent | `Vec<u64>` | 1 Year (`6,307,200` ledgers) | Bounded by user wrap count | List of periods owned by user to enable O(1) balance lookups and transfer updates. |
| `TransferFee` | Instance | `TransferFeeConfig` | Contract Instance Lifetime | ~72 bytes | Token fee configuration (`amount`, `recipient`, `token`) for `transfer_wrap`. |
| `TransferGuard` | Temporary | `bool` | Transaction Lifespan | 1 byte | Ephemeral reentrancy guard for wrap transfers. |
| `MigrationVersion` | Instance | `u32` | Contract Instance Lifetime | 4 bytes | Tracks applied schema/storage migration version. |
| `UserPeriods(Address)` | Persistent | `Vec<u64>` | 1 Year (`6,307,200` ledgers) | Bounded by user history | Historical mint period record for each user. |
| `TotalWrapCount` | Persistent | `u32` | 1 Year (`6,307,200` ledgers) | 4 bytes | Global count of all wraps minted across all users. |
| `TotalRevoked` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Global counter of revoked wrap records. |
| `AliasHash(Address)` | Persistent | `BytesN<32>` | 1 Year (`6,307,200` ledgers) | 32 bytes | User-configured privacy-preserving profile alias hash. |
| `Name` | Instance | `String` | Contract Instance Lifetime | Bounded (≤ 32 bytes) | Display name override for wrap token registry. |
| `Symbol` | Instance | `String` | Contract Instance Lifetime | Bounded (≤ 12 bytes) | Display symbol override for wrap token registry. |
| `Paused` | Instance | `bool` | Contract Instance Lifetime | 1 byte | Global protocol emergency circuit breaker flag. |
| `ExpirationDuration` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Expiration period (seconds) for unverified wraps (default: 7 days). |
| `OptOut(Address)` | Persistent | `bool` | 1 Year (`6,307,200` ledgers) | 1 byte | User-controlled opt-out flag preventing future mints. |
| `LastUpdated(Address)` | Persistent | `u64` | 1 Year (`6,307,200` ledgers) | 8 bytes | Ledger timestamp of last user state modification (mint/revoke). |
| `MintGuard(Address)` | Temporary | `bool` | Transaction Lifespan | 1 byte | Per-user reentrancy and double-call guard during mint operations. |
| `StorageBytes` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Conservative on-chain accounting of persistent storage bytes used. |
| `FeeParams` | Instance | `FeeParams` | Contract Instance Lifetime | ~32 bytes | Algorithmic storage rent fee function configuration parameters. |
| `WhitelistRoot` | Instance | `BytesN<32>` | Contract Instance Lifetime | 32 bytes | Merkle root for off-chain allowlist validation. |
| `TimelockDelay` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Mandatory delay (seconds) between scheduling and execution. |
| `TimelockOp(BytesN<32>)` | Persistent | `TimelockOperation` | 1 Year (`6,307,200` ledgers) | ~80 bytes | Scheduled timelock operation record keyed by deterministic op ID. |
| `TimelockOps` | Instance | `Vec<BytesN<32>>` | Contract Instance Lifetime | Bounded queue | List of active timelock operation IDs queued for execution. |
| `BridgeRelayer` | Instance | `Address` | Contract Instance Lifetime | 32 bytes | Authorized relayer address for cross-chain wrap bridge. |
| `BridgeChainStatus(u32)` | Instance | `bool` | Contract Instance Lifetime | 1 byte | Enabled/disabled status for cross-chain destination chain ID. |
| `OutboundBridgeNonce` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Monotonically increasing sequence counter for outbound bridge requests. |
| `OutboundBridgeRequest(u64)`| Persistent | `OutboundBridgeRequest` | 1 Year (`6,307,200` ledgers) | ~120 bytes | Outbound cross-chain bridge transfer request keyed by outbound nonce. |
| `InboundBridgeProcessed(u32, u64)` | Persistent | `bool` | 1 Year (`6,307,200` ledgers) | 1 byte | Replay protection flag for inbound cross-chain requests `(chain, nonce)`. |
| `InboundBridgeRecord(u32, u64)` | Persistent | `InboundBridgeRecord` | 1 Year (`6,307,200` ledgers) | ~80 bytes | Historical record of completed inbound cross-chain wrap. |
| `AdminProposalCount` | Instance | `u64` | Contract Instance Lifetime | 8 bytes | Monotonic counter of DAO admin governance proposals. |
| `AdminProposal(u64)` | Persistent | `AdminProposal` | 1 Year (`6,307,200` ledgers) | ~112 bytes | Governance proposal record keyed by proposal ID. |
| `AdminProposalVote(u64, Address)` | Persistent | `bool` | 1 Year (`6,307,200` ledgers) | 1 byte | Vote record for `(proposal_id, voter)` preventing double-voting. |
| `ContractVersion` | Instance | `u32` | Contract Instance Lifetime | 4 bytes | Monotonic counter incremented on contract WASM upgrades. |
| `Stake(Address)` | Persistent | `StakeRecord` | 1 Year (`6,307,200` ledgers) | ~32 bytes | Staking balance, lock timestamp, and cooldown state for `user`. |
| `StakeConfig` | Instance | `StakeConfig` | Contract Instance Lifetime | ~24 bytes | Staking parameters (`min_stake`, `cooldown_seconds`, `multiplier`, `max_bps`). |
| `TotalStaked` | Instance | `i128` | Contract Instance Lifetime | 16 bytes | Global aggregate staked token balance across all users. |

---

## 3. Known Storage Tier Mismatches & Analysis

The contract has been audited for compliance with the "Instance storage is for bounded configuration only" rule. The following keys require special architectural consideration:

1. **`TimelockOps` (Instance)**:
   - *Current state:* Stored as `Vec<BytesN<32>>` in instance storage.
   - *Analysis:* Acceptable under normal operational bounds since pending admin actions are infrequent and low-volume. However, if governance proposals grow, the active ID list should be migrated to persistent storage or indexed off-chain.

2. **`BridgeChainStatus(u32)` (Instance)**:
   - *Current state:* Stored per chain ID in instance storage.
   - *Analysis:* Bounded because the number of connected external blockchains is small (typically < 20). Acceptable in instance storage.

3. **`TotalRevoked` (Instance)**:
   - *Current state:* Stored as a single `u64` in instance storage.
   - *Analysis:* Single scalar integer, bounded size (8 bytes). Minimal rent impact.

4. **`Name` & `Symbol` (Instance)**:
   - *Current state:* Stored as small `String` overrides in instance storage.
   - *Analysis:* Metadata overrides are strictly bounded in length (≤ 32 characters) and configured only by admin.

5. **`TotalWrapCount` (Persistent)**:
   - *Current state:* Stored in persistent storage.
   - *Analysis:* Could technically reside in instance storage as a single scalar, but storing in persistent storage isolates counter writes from the shared instance entry.

---

## 4. Algorithmic Fee & Storage Accounting

The contract maintains a conservative estimate of persistent storage bytes in `DataKey::StorageBytes` (instance storage) to price operations fairly based on on-chain state rent consumption.

### Dynamic Fee Formula
$$\text{fee} = \min\left(\text{max\_fee},\, \text{base\_fee} + \text{per\_kib\_fee} \times \left\lceil \frac{\text{storage\_bytes}}{1024 \times \text{scale\_step\_kib}} \right\rceil\right)$$

### Accounting Rules
- `mint_wrap()`: Computes new entries created (`Wrap`, `WrapCount`, `LatestPeriod`, `UserPeriods`, `LastUpdated`) and increments `StorageBytes`.
- `revoke_wrap()`: Decrements `StorageBytes` by the estimated size of the removed `Wrap` and any cleared indexes.
- `extend_ttl()`: Renews TTL for persistent entries without altering byte accounting.
