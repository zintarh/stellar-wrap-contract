# Stellar Wrap Contract v2 Architecture Decisions

This document records product and architecture decisions made as part of the
v2 rewrite of the Stellar Wrap contract. Each entry includes the decision,
rationale, and any follow-up actions.

---

## Decision: Remove `transfer_wrap` (Issue #976)

### Decision

**Remove `transfer_wrap` (and the associated transfer fee infrastructure)
from the contract.** Wrap records are soulbound (non-transferable) and must
remain that way.

### Rationale

1. **Contradicts the soulbound model.** The README and contract documentation
   describe wrap records as non-transferable entries. `transfer_wrap` moves a
   record between addresses, which directly contradicts this guarantee.

2. **Source of index bugs.** `transfer_wrap` has been a steady source of
   index inconsistencies (#656, #657, #729). The transfer operation must keep
   multiple indexes in sync (`WrapPeriods`, `UserPeriods`, `WrapCount`,
   `LatestPeriod`, `LastUpdated`) and any failure mid-flight leaves the
   contract in an inconsistent state. The reentrancy guard (`TransferGuard`)
   and fee-collection ordering add further complexity.

3. **Narrow use case.** The primary use case for `transfer_wrap` is key
   rotation (a user moving to a new wallet). A narrower, purpose-built flow
   (`migrate_owner`) that moves all wraps at once and requires both
   signatures is safer and more explicit.

4. **Unnecessary complexity.** The transfer fee configuration
   (`set_transfer_fee`, `clear_transfer_fee`, `get_transfer_fee`), the
   `TransferGuard` temporary storage key, and the `TransferFeeConfig` type
   are only used by `transfer_wrap`. Removing the entrypoint eliminates all
   of this infrastructure.

### Chosen option

**(a) Remove transfer.** The contract will not support moving individual
wrap records between addresses. If a user needs to move wraps to a new
wallet, they should use `revoke_wrap` on the old wallet (admin-authorized)
and `mint_wrap` on the new wallet, or a future `migrate_owner` flow that
handles bulk migration with dual signatures.

### Follow-up

A follow-up implementation issue should be filed to:

- Remove `transfer_wrap`, `set_transfer_fee`, `clear_transfer_fee`,
  `get_transfer_fee`, `backfill_wrap_periods`, and the `TransferGuard` /
  `TransferFee` storage keys from the contract.
- Remove `TransferFeeConfig` from `storage_types.rs` if no longer needed.
- Remove `transfer.rs` and `transfer_test.rs`.
- Update the event catalogue and public interface documentation.
- Remove the `Transfer` section from the pause table and README.
- Remove `get_transfer_fee` from the expanded query surface in the
  CHANGELOG.

### Alternatives considered

- **(b) Replace with `rotate_owner(old, new)`**: A bulk migration that
  moves all wraps at once and requires both signatures. This was considered
  but deferred because the simpler removal (a) eliminates the entire class
  of transfer-related bugs and keeps the contract surface minimal. A
  `migrate_owner` flow can be introduced later if the need is demonstrated.
- **(c) Keep as-is, justified in docs**: Rejected because the contradiction
  between "soulbound" and "transferable" is a fundamental design flaw that
  causes real bugs, not just documentation confusion.
