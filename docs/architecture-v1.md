# Architecture v1 (deployed today)

This document describes the v1 contract as it is deployed today, before the v2
rewrite lands. It is the agreed baseline for the rewrite RFC and the migration.

## Modules under `src/`

| Module | Responsibility |
| --- | --- |
| `lib.rs` | Contract entry points, wiring of modules, and public API surface. |
| `storage_types.rs` | Shared `#[contracttype]` structs/enums, including `DataKey` and `WrapState`. |
| `storage.rs` | Low-level persistent/temporary storage helpers and key accessors. |
| `mint.rs` | Mint flow: signature verification, wrap creation, counters, events. |
| `queries.rs` | Read-only views (`get_wraps`, `name`, `symbol`, counts, etc.). |
| `transfer.rs` | Wrap ownership transfer, fee collection, and reentrancy guard. |
| `revoke.rs` | Revocation of wrap records and revoked counters. |
| `burn.rs` | Burning of wrap records. |
| `admin.rs` | Admin configuration: admin address, signing key, two-step transfer. |
| `timelock.rs` | Scheduled privileged actions (`TimelockAction`) with delay. |
| `fee.rs` | Algorithmic fee model (`FeeParams`) and fee computation. |
| `bridge.rs` | Outbound/inbound bridge requests and relayer sets. |
| `alias.rs` | Privacy-preserving alias hash registration and lookup. |
| `migration.rs` | Storage migration versioning (`MigrationVersion`). |
| `events.rs` | Event emission helpers used across modules. |
| `errors.rs` | Contract error codes. |

## Mint flow

```
backend (off-chain)
  └─ signs payload {user, period, archetype, data_hash, payload_version, valid_until}
        │  Ed25519 signature (64 bytes)
        ▼
user submits mint(user, period, archetype, data_hash, payload_version, valid_until, signature)
        │
        ▼
contract verifies
  ├─ AdminPubKey present
  ├─ valid_until >= ledger timestamp
  ├─ Ed25519 verify(signature, payload, AdminPubKey)
  └─ period not already minted for user
        │
        ▼
storage writes
  ├─ Wrap(user, period)        = WrapRecord { lifecycle: Draft→Active, ... }
  ├─ WrapCount(user)          += 1
  ├─ LatestPeriod(user)        = period
  ├─ WrapPeriods(user)         += period
  ├─ UserPeriods(user)         += period (insertion order)
  └─ TotalWrapCount           += 1
        │
        ▼
events
  └─ emit mint event {user, period, archetype, data_hash}
```

## `DataKey` variants

Every variant of `DataKey` (see `src/storage_types.rs`) is listed below with its
tier, writer, reader, and TTL policy. TTL policy follows the contract's
`extend_ttl` calls; entries marked *persistent* are extended on access, entries
marked *temporary* expire unless refreshed.

| Variant | Tier | Writer | Reader | TTL policy |
| --- | --- | --- | --- | --- |
| `Admin` | Persistent | `admin::set_admin`, `timelock::execute` | `admin`, `mint`, `timelock` | Extended on write/read |
| `AdminPubKey` | Persistent | `admin::set_admin_pub_key`, `timelock::execute` | `mint` | Extended on write/read |
| `PendingAdmin` | Persistent | `admin::propose_admin` | `admin::accept_admin` | Extended on write/read |
| `Wrap(Address, u64)` | Persistent | `mint`, `transfer`, `revoke`, `burn` | `queries`, `transfer`, `revoke`, `burn` | Extended on write/read |
| `WrapCount(Address)` | Persistent | `mint`, `revoke`, `burn` | `queries` | Extended on write/read |
| `LatestPeriod(Address)` | Persistent | `mint`, `transfer` | `queries`, `transfer` | Extended on write/read |
| `WrapPeriods(Address)` | Persistent | `mint`, `transfer` | `transfer` | Extended on write/read |
| `TransferFee` | Persistent | `admin::set_transfer_fee` | `transfer` | Extended on write/read |
| `TransferGuard` | Temporary | `transfer` (set/clear) | `transfer` | Short-lived; cleared after call |
| `MigrationVersion` | Persistent | `migration::migrate` | `migration` | Extended on write/read |
| `UserPeriods(Address)` | Persistent | `mint` | `queries::get_wraps` | Extended on write/read |
| `TotalWrapCount` | Persistent | `mint` | `queries` | Extended on write/read |
| `TotalRevoked` | Persistent | `revoke` | `queries` | Extended on write/read |
| `AliasHash(Address)` | Persistent | `alias::set_alias` | `alias::get_alias` | Extended on write/read |
| `AliasOwner(BytesN<32>)` | Persistent | `alias::set_alias` | `alias::set_alias` | Extended on write/read |
| `Name` | Persistent | `admin::set_name` | `queries::name` | Extended on write/read |
| `Symbol` | Persistent | `admin::set_symbol` | `queries::symbol` | Extended on write/read |

> Note: the table above must be kept in sync with `src/storage_types.rs`. Any
> new `DataKey` variant added before v2 must be added here as well.
