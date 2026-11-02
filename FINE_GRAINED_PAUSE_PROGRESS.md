# Fine-Grained Pause Implementation Progress

## Issue Reference
Part of #649 - Replace single boolean `Paused` flag with fine-grained `PauseFlags`

## Problem Statement
The current contract uses a single boolean `Paused` flag that stops ALL operations. During incidents (e.g., bridge issues), operators must pause everything including unrelated operations like minting, causing unnecessary user disruption.

## Solution Approach
Replace `Paused` with bitflag `PauseFlags { mint, transfer, revoke_burn, bridge, ttl, stake }` to enable selective pausing.

## Progress: ~1% Complete

### ✅ What's Been Implemented

1. **Storage Infrastructure** (`src/storage_types.rs`)
   - Added `DataKey::PauseFlags` storage key
   - Defined `PauseFlags` struct with 6 operation categories:
     - `mint`: mint_wrap, mint_wrap_batch
     - `transfer`: transfer_wrap
     - `revoke_burn`: revoke_wrap, burn_wrap
     - `bridge`: bridge_wrap_in, bridge_wrap_out
     - `ttl`: extend_ttl operations
     - `stake`: stake, unstake, withdraw_stake
   - Added helper methods: `none()`, `all()`, `is_any_paused()`

2. **Admin Functions** (`src/admin.rs`)
   - `read_pause_flags(e: &Env) -> PauseFlags` - Read flags from storage
   - `write_pause_flags(e: &Env, flags: &PauseFlags)` - Write flags to storage
   - `require_mint_not_paused(e: &Env)` - Check if mint is paused
   - `require_bridge_not_paused(e: &Env)` - Check if bridge is paused

3. **Tests** (`src/fine_grained_pause_test.rs`)
   - Basic storage read/write tests
   - Individual flag blocking behavior tests
   - Selective pause demonstration (mint paused, bridge allowed)
   - Documents intended behavior for future integration

4. **Type Export** (`src/lib.rs`)
   - Exported `PauseFlags` for public API access

### 🚧 What's NOT Yet Implemented (Remaining 99%)

1. **Entrypoint Integration**
   - Replace `require_not_paused()` calls with specific flag checks in:
     - [ ] `mint.rs` → `require_mint_not_paused()`
     - [ ] `bridge.rs` → `require_bridge_not_paused()`
     - [ ] `transfer.rs` → `require_transfer_not_paused()`
     - [ ] `stake.rs` → `require_stake_not_paused()`
     - [ ] `revoke.rs` → `require_revoke_burn_not_paused()`
     - [ ] `ttl.rs` → `require_ttl_not_paused()`

2. **Admin Entrypoints**
   - [ ] Add `pause(flags: PauseFlags)` - Set specific flags
   - [ ] Add `unpause(flags: PauseFlags)` - Clear specific flags
   - [ ] Add `paused_flags() -> PauseFlags` - Query current flags
   - [ ] Update `is_paused()` to return `flags.is_any_paused()` for backwards compatibility

3. **Remaining Helper Functions**
   - [ ] `require_transfer_not_paused()`
   - [ ] `require_stake_not_paused()`
   - [ ] `require_revoke_burn_not_paused()`
   - [ ] `require_ttl_not_paused()`

4. **Comprehensive Testing**
   - [ ] Table test mapping every write entrypoint to its flag
   - [ ] Coverage test ensuring each flag blocks its operations
   - [ ] Integration test: pause bridge → mint still works
   - [ ] Backwards compatibility test for `is_paused()`
   - [ ] Event emission tests for flag changes

5. **Migration Strategy**
   - [ ] Define migration path from boolean `Paused` to `PauseFlags`
   - [ ] Add migration function to convert existing pause state
   - [ ] Deprecation warnings for old `pause()`/`unpause()`

6. **Documentation**
   - [ ] Update docs/incident-runbook.md with flag usage
   - [ ] Document each flag's scope in API docs
   - [ ] Add examples for common pause scenarios

## Architecture Notes

### Design Decisions
- **Backwards Compatibility**: Keep `DataKey::Paused` initially, migrate later
- **Error Handling**: Reuse existing `ContractError::Paused` for all flag checks
- **Storage Layout**: Store as struct (not bitfield) for clarity and debuggability
- **Default State**: `PauseFlags::none()` when unset (all operations allowed)

### Flag Semantics
- `true` = operation IS paused (blocked)
- `false` = operation is allowed
- Any flag set → `is_any_paused()` returns `true`

## Next Steps (Priority Order)

1. **Add remaining helper functions** (`require_*_not_paused` for each category)
2. **Integrate into one entrypoint** (e.g., mint_wrap) as proof of concept
3. **Add entrypoint mapping table test** (every write op → exactly one flag)
4. **Systematically replace all `require_not_paused` calls**
5. **Add admin entrypoints for flag manipulation**
6. **Comprehensive integration testing**
7. **Migration strategy and backwards compatibility**

## Testing Strategy

### Unit Tests (Done)
- ✅ Flag storage read/write
- ✅ Individual flag checks
- ✅ Selective pause behavior

### Integration Tests (TODO)
- Pause mint → bridge still works (#649 bug scenario)
- Pause bridge → mint still works (main use case)
- All flags → behaves like old `Paused`
- No flags → all operations work

### Coverage Tests (TODO)
- Table test: entrypoint → flag mapping
- Ensure every write operation checks exactly one flag

## File Changes Summary

- `src/storage_types.rs`: Added `PauseFlags` struct and `DataKey::PauseFlags`
- `src/admin.rs`: Added flag read/write and check functions
- `src/lib.rs`: Exported `PauseFlags` and included test module
- `src/fine_grained_pause_test.rs`: NEW - Foundational tests
- `src/pause_flags_test.rs`: Basic struct tests

## Estimated Completion

- **Done**: ~1% (infrastructure + basic tests)
- **Remaining**: ~99% (entrypoint integration + full testing + migration)
- **Next milestone**: 10% (all helper functions + one entrypoint integrated)
