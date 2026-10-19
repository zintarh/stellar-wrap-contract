# Architecture v1 (deployed today)

This document is the baseline map of the v1 system as it is deployed today, before the v2 rewrite lands. It is intended to give contributors and the migration an agreed reference for modules, storage keys, roles, and call flows.

## Modules under `src/`

| Module | Responsibility |
| --- | --- |
| `src/lib.rs` | Crate entry point; wires up the contract and re-exports the public surface. |
| `src/storage_types.rs` | Defines `DataKey` and the on-chain storage layout used by the contract. |
| `src/storage_fee_test.rs` | Tests covering storage fee / TTL behaviour of the storage keys. |
| `src/contract.rs` | Core contract logic: mint submission, verification, and storage writes. |
| `src/events.rs` | Emits contract events for state changes (mint, verification, storage writes). |
| `src/roles.rs` | Role definitions and authorization checks (admin, backend signer, user). |

> If a module is added or renamed, update this table so the v1 baseline stays accurate.

## Mint flow

1. **Backend signs** — the trusted backend produces a signed mint authorization for a user.
2. **User submits** — the user submits the signed authorization to the contract.
3. **Contract verifies** — the contract verifies the backend signature and authorization contents.
4. **Storage writes** — on success, the contract writes the relevant `DataKey` entries (see table below).
5. **Events** — the contract emits events describing the mint and the storage changes.

```
backend signs -> user submits -> contract verifies -> storage writes -> events
```

## `DataKey` variants

Every `DataKey` variant defined in `src/storage_types.rs` is listed below with its storage tier, writer, reader, and TTL policy.

| `DataKey` variant | Tier | Writer | Reader | TTL policy |
| --- | --- | --- | --- | --- |
| `DataKey::Admin` | Persistent | Admin (setup) | Contract (auth checks) | No expiry; persistent. |
| `DataKey::BackendSigner` | Persistent | Admin (setup) | Contract (verification) | No expiry; persistent. |
| `DataKey::MintRecord(user)` | Persistent | Contract (mint flow) | Contract / user queries | Extended on write; persistent. |
| `DataKey::UserBalance(user)` | Persistent | Contract (mint flow) | Contract / user queries | Extended on write; persistent. |
| `DataKey::Config` | Instance | Admin (setup) | Contract (all flows) | Tied to contract instance lifetime. |

> Keep this table in sync with `src/storage_types.rs`: every `DataKey` variant must appear here. See `STORAGE.md` for the detailed storage reference.
