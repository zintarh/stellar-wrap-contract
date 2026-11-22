# Stellar Wrap v2 — Architecture Decisions

This document records architecture decisions for the Stellar Wrap v2 rewrite.
Each section states the decision, the reasoning, and the consequences for the
codebase.

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

## Decision: remove `verify_with_oracle` from the registry

**Status:** Accepted (closes #647, tracked by #1019)

### Context

`verify_with_oracle(oracle, data_hash)` accepted an arbitrary oracle address
from the caller and forwarded a cross-contract call to it. The function was
read-only and did not depend on any registry state, so any client could call
the oracle directly. A registry entrypoint that forwards calls to
caller-chosen contracts adds attack surface and nothing else.

### Decision

- The `verify_with_oracle` entrypoint is removed from the registry.
- `src/oracle.rs` (the `DataHashOracle` client, `OracleError` and the
  instruction-budget helper) is deleted along with its module declaration and
  its test module.
- The registry invokes **no caller-supplied contract address**. The only
  remaining cross-contract calls are token transfers to the token contract
  configured by the admin.

### Consequences

- Clients that need an oracle verdict call the oracle contract directly; the
  registry no longer proxies it.
- If an oracle check is wanted at mint time, the oracle address must be
  admin-configured and invoked inside `mint_wrap`, never taken from a call
  argument.
- `verify_data` remains the on-chain hash check (see `docs/verify-data.md`).
- The removal is recorded in `CHANGELOG.md`.
