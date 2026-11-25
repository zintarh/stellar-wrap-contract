# Stellar Wrap v2 — Architecture Decisions

This document records architecture decisions for the Stellar Wrap v2 rewrite.
Each section states the decision, the reasoning, and the consequences for the
codebase.

## Decision: freezeable registry with timelocked upgrades

**Status:** Accepted

### Options considered

1. Keep every contract upgradeable through the governor and timelock forever.
2. Keep the registry upgradeable during the v2 migration, then permanently
   disable upgrades with a one-way `freeze_upgrades()` switch.
3. Deploy an immutable registry from day one.

Option 2 is selected. It preserves a controlled migration window without
requiring registry users to trust an upgrader indefinitely.

### Policy per contract

| Contract | Upgrade authority | Delay | Immutability policy |
| --- | --- | --- | --- |
| `wrap-registry` | `Upgrader` role; the production holder is the governor | Configured timelock delay once the timelock is enabled | `freeze_upgrades` is one-way; after execution no direct or queued upgrade can succeed |
| `wrap-bridge` | Governor | Timelock required | Remains upgradeable because chain adapters and relayer policy can change |
| Governor/timelock | Deployment-specific governance authority | Governance voting period plus timelock delay | Remains upgradeable until a separate governance RFC removes that capability |
| Optional periphery (indexer, signer, fee modules) | Component owner/governor | Component-specific operational delay | Upgradeable; it cannot mutate frozen registry code |

Both `upgrade` and `freeze_upgrades` are direct `Upgrader` operations before
the timelock is enabled. After it is enabled, they are available only through
`TimelockAction::Upgrade` and `TimelockAction::FreezeUpgrades`. Freezing also
invalidates upgrades that were queued earlier but executed later.

Every successful upgrade increments `contract_version` exactly once and emits
the canonical `("upgrade", version)` event with the new WASM hash. The freeze
emits `("upgrade", "frozen")`. There is deliberately no unfreeze entrypoint.

## Decision: remove staking and the algorithmic fee model from the registry

**Status:** Accepted

### Context

The registry crate (`contracts/registry`) accumulated two pieces of logic that
no product surface depends on:

- `src/stake.rs` — stake records and staking entrypoints, originally added in
  #636 without an accompanying token transfer.
- `src/storage_accounting.rs` plus `FeeParams`, `current_fee`,
  `get_discounted_fee` and `TransferFeeConfig` — an algorithmic fee model that
  computes a fee but never charges it (#666).

Both cost WASM size, storage reads on the mint path and audit time while
providing no user-visible behaviour.

### Who pays fees, for what action

Under the current model, nobody pays a fee. The algorithmic fee is computed
from `FeeParams` but is never transferred, so it is dead code. If fees are
introduced in a future version, the intended payer is the account performing a
mint or transfer of a wrapped asset, and the fee would be charged on that
single action. Staking, as originally conceived, would have bought a user a
fee discount — but since no fee is ever charged, staking buys nothing today.

### Decision

Rather than move this logic into a separate `contracts/wrap-fees` contract, we
**remove** it from the registry:

- `src/stake.rs` and `src/storage_accounting.rs` are deleted along with their
  module declarations.
- `FeeParams`, `current_fee`, `get_discounted_fee`, `TransferFeeConfig` and the
  related `DataKey` variants and public entrypoints are removed.
- `storage_bytes` accounting is removed from the hot mint path, so a mint
  performs no staking or fee storage reads.

### Why removal over a separate contract

- **No product purpose.** A `wrap-fees` contract would still have no caller and
  no payer until a fee product is actually specified. Moving dead code does not
  make it live; it only relocates the audit and maintenance cost.
- **Smaller attack surface.** Removing the modules shrinks the registry WASM and
  eliminates storage reads that were never needed on the mint path.
- **Cheaper to reintroduce than to carry.** If a fee product is defined later,
  it can be designed against real requirements (who pays, for what action, how
  staking interacts) instead of inheriting an unused model. A dedicated
  `wrap-fees` contract remains an option at that point, with the registry
  calling `quote_fee(user, action)` once per mint or transfer.

### Consequences

- The registry mint path performs no staking or fee storage reads.
- `docs/staking.md` describes behaviour that no longer exists and should be
  treated as historical context until a fee product is specified.
- The removal is recorded in `CHANGELOG.md`.
- Tests for staking and fee computation are removed with the modules; the
  remaining registry tests cover mint, transfer and metadata behaviour.
