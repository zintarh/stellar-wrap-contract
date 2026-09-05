# Architecture Decision: Upgradeable Proxy Pattern vs Native Upgrades

## Context
Issue #522 requested the refactoring of the contract to use the "upgradeable proxy pattern for seamless versioning". This pattern is common in EVM-based blockchains.

## Analysis of Stellar's State Model
In Soroban (Stellar's smart contract platform), the execution and storage model differs significantly from EVM:
1. **No `delegatecall` equivalent:** Soroban does not support executing another contract's logic within the current contract's storage context.
2. **Storage tied to Contract ID:** If a "Proxy" contract uses `env.invoke_contract()` to forward calls to an "Implementation" contract, the execution context switches. The Implementation contract will read and write to its *own* storage, not the Proxy's storage.
3. **Loss of State on Upgrade:** In a proxy setup, upgrading the implementation would require deploying a new logic contract (with a new Contract ID). Because storage is tied to the Contract ID, the new logic contract would have empty storage. State migration would be prohibitively expensive and complex.

## Decision
Implementing an EVM-style upgradeable proxy pattern is an **anti-pattern in Soroban** and would break the contract's ability to maintain continuous state across version upgrades.

Instead, Soroban provides a **native, secure upgrade mechanism**:
`env.deployer().update_current_contract_wasm(new_wasm_hash)`

This native capability is already fully implemented in `src/admin.rs` (`upgrade`function). It allows the contract logic (WASM) to be updated in-place while retaining the identical Contract ID and all existing persistent/instance storage.

## Batch Proxy for Atomic `mint_wrap`
To satisfy the requirement of atomically batching multiple `mint_wrap` calls, a new `BatchProxy` contract has been introduced. This proxy is **not** an upgradeable proxy and does not alter the storage ownership model.

- The `BatchProxy` receives a list of `mint_wrap` arguments and invokes the main contract's `mint_wrap` function sequentially via `env.invoke_contract(`.
- Because all invocations happen within a single Soroban transaction, they are atomic: if any call fails, the entire transaction reverts.
- The `BatchProxy` maintains no state of its own; it simply forwards the calls to the canonical contract ID. This avoids any conflicts with the main contract's persistent storage.

## Conclusion
No Rust code refactoring was performed to introduce an upgradeable Proxy struct, as doing so would compromise the contract's state model. The existing native upgrade pattern (`admin::upgrade`) is the correct and canonical architecture for seamless versioning in Soroban.

For the batching requirement, a separate `BatchProxy` contract has been added. It is intentionally minimal and does not interfere with the main contract's storage or upgrade mechanism. The atomicity is guaranteed by the Soroban transaction model.
