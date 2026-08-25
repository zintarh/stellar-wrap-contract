# Storage key reference

This document is the upgrade reference for maintainers of the Stellar Wrap contract. The source of truth for key names and payloads is `src/storage_types.rs`; namespace assignments below are based on the `e.storage()` calls in `src/`.

Soroban has three storage namespaces:

- **Instance**: contract-wide state. Entries share the contract instance TTL.
- **Persistent**: durable, addressable state. Each entry has its own TTL and can expire independently.
- **Temporary**: short-lived state. Each entry has its own TTL and is intended for ephemeral or reconstructible data.

## DataKey map

| Variant | Namespace | Stored value / key scope |
| --- | --- | --- |
| `Admin` | Instance | Current admin `Address` |
| `AdminPubKey` | Instance | Ed25519 signing key `BytesN<32>` |
| `PendingAdmin` | Instance | Proposed admin `Address` |
| `Wrap(Address, u64)` | Persistent | `WrapRecord`, keyed by user and period |
| `WrapCount(Address)` | Persistent | Per-user wrap count |
| `LatestPeriod(Address)` | Persistent | Per-user latest period |
| `WrapPeriods(Address)` | Persistent | Per-user periods used by transfer indexing |
| `TransferFee` | Instance | `TransferFeeConfig` |
| `TransferGuard` | Temporary | Transfer reentrancy guard |
| `MigrationVersion` | Instance | Highest applied storage migration version |
| `StorageSchemaVersion` | Instance | Storage schema version initialized at deployment |
| `UserPeriods(Address)` | Persistent | Per-user minted period list |
| `TotalWrapCount` | Persistent | Global successful mint count |
| `TotalRevoked` | Temporary | Global revoked count; defaults to `0` when absent |
| `AliasHash(Address)` | Persistent | User-controlled alias hash |
| `Name` | Temporary | Optional display name; defaults when absent |
| `Symbol` | Temporary | Optional display symbol; defaults when absent |
| `Paused` | Instance | Emergency pause flag |
| `ExpirationDuration` | Instance | Unverified-wrap expiration duration in seconds |
| `OptOut(Address)` | Persistent | Presence means the user opted out of future mints |
| `LastUpdated(Address)` | Persistent | Per-user last mint/revoke ledger timestamp |
| `MintGuard(Address)` | Temporary | Per-user mint reentrancy guard |
| `StorageBytes` | Instance | Estimated persistent storage bytes |
| `FeeParams` | Instance | Algorithmic fee `FeeParams` |
| `WhitelistRoot` | Instance | Off-chain whitelist Merkle root |
| `TimelockDelay` | Instance | Timelock delay in seconds |
| `TimelockOp(BytesN<32>)` | Persistent | Scheduled `TimelockOperation`, keyed by operation ID |
| `TimelockOps` | Instance | IDs of currently scheduled timelock operations |
| `BridgeRelayer` | Instance | Authorized bridge relayer `Address` |
| `BridgeChainStatus(u32)` | Instance | Enabled flag for a chain ID |
| `OutboundBridgeNonce` | Instance | Outbound bridge sequence counter |
| `OutboundBridgeRequest(u64)` | Persistent | Outbound request, keyed by nonce |
| `InboundBridgeProcessed(u32, u64)` | Persistent | Processed flag, keyed by source chain and nonce |
| `InboundBridgeRecord(u32, u64)` | Persistent | Inbound record, keyed by source chain and nonce |
| `AdminProposalCount` | Instance | Governance proposal sequence counter |
| `AdminProposal(u64)` | Persistent | Governance proposal, keyed by proposal ID |
| `AdminProposalVote(u64, Address)` | Persistent | Voter choice, keyed by proposal ID and voter |
| `ContractVersion` | Instance | Version incremented by `upgrade` |
| `Stake(Address)` | Persistent | User `StakeRecord` |
| `StakeConfig` | Instance | Admin-configured `StakeConfig` |
| `TotalStaked` | Instance | Global staked amount |

Every variant in the current `DataKey` enum is listed above. Do not infer a namespace from the variant name alone: for example, `TotalWrapCount` is persistent while `AdminProposalCount` is instance storage.

## TTL behavior

### Instance

Instance data is contract-wide and shares one instance TTL. Code paths that mutate bridge state and the public `extend_ttl` path explicitly extend the instance TTL to approximately one year:

```rust
const TTL_ONE_YEAR: u32 = 17_280 * 365;
```

Other instance writes do not create per-key TTLs. A maintainer changing an instance write must preserve the instance-renewal policy and verify that the contract instance cannot expire while the contract is in active use.

### Persistent

Persistent entries have independent TTLs. Wrap records, per-user metadata, bridge records, timelock operations, stakes, aliases, opt-outs, and last-updated records are written with an approximately one-year TTL in their owning code paths (`17_280 * 365`). A persistent entry is not renewed merely because another entry for the same user is written.

The public `extend_ttl(user, period)` renews the selected `Wrap`, plus the user's `WrapCount` and `LatestPeriod`, when present. It does not enumerate or renew every historical wrap. `renew_all_ttls(user)` renews metadata only. Callers that require historical wrap availability must renew each period.

Governance proposal and vote writes currently use persistent storage without a local explicit TTL extension. Treat those entries as independently expirable and preserve or deliberately change that behavior as part of an upgrade.

### Temporary

Temporary entries are independently expirable and must not be used for canonical durable records. `Name` and `Symbol` are explicitly renewed to approximately one day (`17_280` ledgers); when absent, queries return their hardcoded defaults. `TotalRevoked` is informational and also defaults to zero when absent.

`MintGuard(Address)` and `TransferGuard` are reentrancy guards. They are removed on successful completion; their temporary lifetime prevents a failed call from permanently blocking future calls. Preserve this behavior when changing guard code.

## Upgrade compatibility

An upgrade replaces the WASM, not the contract's storage. Existing keys remain readable only when the new code preserves their Soroban `contracttype` encoding, namespace, and value type.

For any upgrade that touches storage:

1. Treat the `DataKey` variant order and payload types as serialized ABI. Do not reorder variants, remove variants, rename variants, or change the fields of an existing variant. Add new variants only at the end.
2. Keep each existing variant in the same storage namespace. Moving a key from instance to persistent or temporary storage creates a different key space; it does not migrate the old value.
3. Keep the stored value's contract type compatible. For an intentional schema change, add a new key/version and migrate explicitly rather than decoding old bytes as a new type.
4. Use `MigrationVersion` for one-time, admin-authorized migrations. Migrations must be monotonic and safe to retry, or must fail before partial state is committed.
5. Preserve TTL expectations. A migration that rewrites persistent data must renew each rewritten entry; it must not assume that renewing one key renews related keys.
6. Before deployment, compare the new `DataKey` enum and all namespace calls with this reference, then test reads of representative pre-upgrade data in an upgrade simulation.

`StorageSchemaVersion` is initialized to `CURRENT_STORAGE_SCHEMA_VERSION` (`1`)
and is exposed through `storage_schema_version()`. An upgrade that preserves the
existing storage encoding must preserve this value. An upgrade that changes the
schema must add and execute an explicit, admin-authorized migration before
publishing the new schema version. Keep `MigrationVersion` for tracking the
individual migration operations; it is separate from the resulting schema
version. Deployments from before this key was introduced read as version `0`
until a deliberate compatibility migration establishes their actual schema.

The canonical serialized representation is produced by Soroban's `#[contracttype]` encoding. Do not hand-roll or depend on a guessed byte layout.
