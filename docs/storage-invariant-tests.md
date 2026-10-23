# Storage Invariant Tests Implementation

## Task #272: Security - Add storage invariant tests for direct state consistency

### Implementation Summary

This implementation adds comprehensive storage invariant tests that directly inspect contract storage to verify core consistency properties after mixed mint and revoke flows.

### Files Added

1. **`src/storage_invariants_test.rs`** - New test module with comprehensive storage invariant tests

### Files Modified

1. **`src/lib.rs`** - Added `storage_invariants_test` module declaration
2. **`src/alias.rs`** - Fixed doc comment placement (module-level doc comments must come before imports)
3. **`src/optout.rs`** - Fixed typo in import (`soroban_sd` → `soroban_sdk`)

### Key Features

#### StorageInspector Utility

A test utility struct that provides direct storage access for verification:

- `get_wrap_count()` - Reads `WrapCount` directly from storage
- `get_total_wrap_count()` - Reads `TotalWrapCount` directly
- `get_latest_period()` - Reads `LatestPeriod` directly
- `get_user_periods()` - Reads `UserPeriods` vector directly
- `get_wrap_periods()` - Reads `WrapPeriods` vector directly
- `has_wrap()` - Checks wrap existence without going through contract API
- `count_live_wraps()` - Counts actual live wraps by iterating periods
- `all_user_periods_have_wraps()` - Verifies period index integrity
- `latest_period_is_valid()` - Verifies LatestPeriod points to existing wrap
- `latest_period_is_max()` - Verifies LatestPeriod is the maximum period with a live wrap
- `periods_are_synchronized()` - Verifies UserPeriods and WrapPeriods are in sync

#### Core Invariants Tested

1. **WrapCount matches live wraps**: Verifies that `WrapCount` always equals the actual number of live wrap records

2. **LatestPeriod references existing wrap**: Verifies that when `LatestPeriod` is set, it always points to an actual wrap record

3. **LatestPeriod is recomputed after revoke**: Tests the documented exception where revoking the newest wrap correctly recomputes `LatestPeriod` to point to the next-newest remaining wrap

4. **Period indexes stay synchronized**: Verifies `UserPeriods` and `WrapPeriods` remain consistent

5. **TotalWrapCount matches sum**: Verifies global wrap count matches the sum of all user counts

#### Test Coverage

**Core Invariant Tests:**
- `test_invariant_wrap_count_matches_live_wraps` - Basic wrap counting
- `test_invariant_wrap_count_decrements_after_revoke` - Count decrements correctly
- `test_invariant_latest_period_references_existing_wrap` - LatestPeriod validity
- `test_invariant_latest_period_after_revoking_newest` - LatestPeriod recomputation
- `test_invariant_latest_period_after_revoking_middle_wrap` - LatestPeriod unchanged when non-latest revoked
- `test_invariant_user_periods_all_have_live_wraps_after_revoke` - Period index consistency
- `test_invariant_periods_synchronized` - UserPeriods/WrapPeriods synchronization
- `test_invariant_total_wrap_count_across_users` - Multi-user count aggregation
- `test_invariant_total_wrap_count_decrements_after_revoke` - Global count decrements

**Mixed Flow Tests:**
- `test_invariants_after_complex_mint_revoke_flow` - Complex sequential operations
- `test_invariants_multi_user_interleaved_operations` - Interleaved multi-user operations
- `test_invariants_revoke_all_then_mint_new` - Full cycle: mint → revoke all → mint again

### Known Invariant Exceptions

As documented in the code:

**LatestPeriod after revoking newest wrap**: When the newest wrap is revoked, `LatestPeriod` is recomputed to point to the next-newest remaining period. If all wraps are revoked, `LatestPeriod` is cleared (becomes `None`). This is the expected behavior and is explicitly tested.

### Testing Approach

The tests use direct storage access via `env.as_contract()` to:

1. Read storage keys directly without going through the contract API
2. Verify internal consistency that may not be exposed through public methods
3. Catch subtle bugs in storage updates that could lead to inconsistent state

This approach complements API-level tests by verifying the actual storage layout and ensuring no invariant violations occur at the storage layer.

### Compilation Status

⚠️ **Note**: The codebase has pre-existing compilation errors in `src/bridge.rs` and `src/merkle.rs` that appear to be corrupted AI-generated content. These errors prevent compilation and testing of the new invariant tests.

The corruption appears in both the local repository and the origin/main branch, suggesting it was introduced in a previous commit.

### Next Steps

1. **Fix corrupted files**: `src/bridge.rs` and `src/merkle.rs` need to be restored or rewritten
2. **Run tests**: Once compilation succeeds, run the full test suite with:
   ```bash
   cargo test --lib storage_invariants_test
   ```
3. **Verify coverage**: Ensure all invariants pass after mixed mint/revoke operations
4. **Integration**: Consider expanding tests to cover bridge-in operations and batch mints

### Acceptance Criteria Status

- ✅ Added tests that inspect storage after mixed mint and revoke flows
- ✅ Documented known invariant exception (latest after revoking newest)
- ✅ Used contract testutils without exposing internals in production API
- ⚠️ Cannot verify tests pass due to pre-existing compilation errors in main branch

### References

- Referenced files as specified: `src/storage_types.rs` (empty placeholder), `src/test.rs`
- Storage access patterns from `src/mint.rs`, `src/revoke.rs`, `src/burn.rs`
- Test utilities from `src/test_utils.rs`
