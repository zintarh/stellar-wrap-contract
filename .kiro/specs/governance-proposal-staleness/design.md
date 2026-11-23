# Governance Proposal Staleness Bugfix Design

## Overview

A governance proposal records a `proposed_admin` address at creation time, but nothing binds the
proposal to the admin identity that was in effect when it was created. If the contract admin
changes between proposal creation and execution — through `update_admin`, through a timelocked
`SetAdmin` operation, or through a prior governance proposal execution — the original proposal
remains executable and will silently overwrite the current admin with a superseded address.

The fix is minimal and surgical: snapshot the current `Admin` address onto each `AdminProposal`
at creation time (new field `admin_at_creation`), then reject execution with `StaleProposal`
whenever the current admin no longer matches that snapshot. No other proposal logic, voting
mechanics, or timelock integration is changed.

---

## Glossary

- **Bug_Condition (C)**: The condition `current_admin ≠ proposal.admin_at_creation` — the admin
  stored in contract state has drifted from the address that was in effect when the proposal was
  created.
- **Property (P)**: For any proposal where C holds, `execute_admin_proposal` SHALL return
  `Err(ContractError::StaleProposal)` and leave `DataKey::Admin` unchanged.
- **Preservation**: All proposal lifecycle behaviour for proposals where C does **not** hold —
  defeat, cancellation, voting guards, timelock routing — must remain identical to the pre-fix
  behaviour.
- **`execute_admin_proposal`**: The function in `src/governance.rs` that finalises a proposal
  after voting ends. It currently writes `proposed_admin` unconditionally if votes pass.
- **`create_admin_proposal`**: The function in `src/governance.rs` that stores a new
  `AdminProposal`. This is where `admin_at_creation` will be captured.
- **`AdminProposal`**: The `#[contracttype]` struct in `src/storage_types.rs` that will gain the
  `admin_at_creation: Address` field introduced by this fix.
- **`admin_at_creation`**: The new immutable snapshot field on `AdminProposal`. It records the
  `DataKey::Admin` value at the moment the proposal was created and is never modified afterwards.
- **`StaleProposal`**: The new `ContractError` variant (discriminant 57) returned by
  `execute_admin_proposal` when the bug condition holds.
- **`DataKey::Admin`**: The instance-storage key that holds the current authoritative admin
  address.

---

## Bug Details

### Bug Condition

The bug manifests when the `DataKey::Admin` value in contract instance storage at execution time
differs from the value it held when the proposal was created. The `execute_admin_proposal`
function does not compare those two values; it writes `proposed_admin` into `DataKey::Admin`
unconditionally whenever `votes_for > votes_against`.

**Formal Specification:**

```
FUNCTION isBugCondition(proposal, env)
  INPUT:  proposal of type AdminProposal
          env      of type contract execution environment
  OUTPUT: boolean

  current_admin   ← env.storage.instance.get(DataKey::Admin)
  creation_admin  ← proposal.admin_at_creation   // field introduced by this fix

  RETURN current_admin ≠ creation_admin
END FUNCTION
```

### Examples

- **Direct `update_admin` before execution**: Admin A creates a proposal proposing address X.
  Admin A immediately rotates to admin B via `update_admin`. The proposal's `admin_at_creation`
  is A; the current admin is B; `isBugCondition` returns `true`.  
  *Current (buggy)*: execution writes X, replacing B.  
  *Fixed*: execution returns `StaleProposal`, B is unchanged.

- **Timelocked `SetAdmin` completes during voting**: Admin A schedules `SetAdmin(B)` with a 1-hour
  delay, then a governance proposal proposing X is created under admin A. The timelock executes
  (setting admin to B) before the proposal's voting period ends. `isBugCondition` returns `true`.  
  *Current (buggy)*: execution writes X, reverting to a superseded state.  
  *Fixed*: execution returns `StaleProposal`.

- **Concurrent-proposal race**: Two proposals are open simultaneously under admin A — proposal 1
  proposes X, proposal 2 proposes Y. Proposal 1 executes first (admin becomes X). When proposal 2
  tries to execute, its `admin_at_creation` is A but the current admin is X; `isBugCondition`
  returns `true`.  
  *Current (buggy)*: proposal 2 executes and sets admin to Y, silently undoing proposal 1.  
  *Fixed*: proposal 2 returns `StaleProposal`.

- **No admin change (happy path)**: Admin A creates a proposal proposing X. No admin change
  occurs. Voting passes. `admin_at_creation` is A; current admin is A; `isBugCondition` returns
  `false`. The proposal executes normally.

---

## Expected Behavior

### Preservation Requirements

**Unchanged Behaviors:**

- A proposal created and executed with no intervening admin change SHALL continue to execute
  successfully, updating the admin to the `proposed_admin` address.
- A proposal with `votes_against ≥ votes_for` SHALL continue to be marked `Defeated` with no
  admin change.
- A cancelled proposal SHALL continue to reject execution with `ProposalNotActive`.
- An already-executed proposal SHALL continue to reject re-execution with `ProposalNotActive`.
- An already-defeated proposal SHALL continue to reject re-execution with `ProposalNotActive`.
- Execution before the voting period ends SHALL continue to fail with
  `ProposalVotingPeriodNotEnded`.
- Voting on an ended proposal SHALL continue to fail with `ProposalVotingPeriodEnded`.
- When the timelock is enabled and a proposal passes (with no admin drift), execution SHALL
  continue to schedule `TimelockAction::SetAdmin` rather than writing `DataKey::Admin` directly.

**Scope:**  
All inputs that do NOT satisfy `isBugCondition` (i.e. where `admin_at_creation` equals the
current admin) must be completely unaffected. This includes:

- Any proposal created and executed under the same admin without any interleaved admin change.
- All vote-casting interactions (`vote_admin_proposal`).
- All cancellation interactions (`cancel_admin_proposal`).
- All direct admin operations (`update_admin`, `propose_admin`, `accept_admin`, timelock
  operations) that do not involve governance proposals.

---

## Hypothesized Root Cause

Based on the bug description and source-code review:

1. **Missing creation-time snapshot**: `create_admin_proposal` in `src/governance.rs` constructs
   an `AdminProposal` without capturing `read_admin(&e)`. The `AdminProposal` struct (defined in
   `src/storage_types.rs`) has no field for the creation-time admin, so there is nothing to
   compare against at execution time.

2. **Unconditional write in `execute_admin_proposal`**: The passing branch of
   `execute_admin_proposal` writes `proposal.proposed_admin` into `DataKey::Admin` (or schedules
   `SetAdmin` via the timelock) without first verifying that the admin has not changed since the
   proposal was created. The guard only checks `proposal.status == Active` and
   `now > proposal.end_time`.

3. **Three independent admin-change routes**: `docs/timelock.md` and `docs/authority-model.md`
   document that `DataKey::Admin` can be modified via `update_admin` (direct, immediate),
   `timelock_execute` with `TimelockAction::SetAdmin` (delayed), or `execute_admin_proposal`
   itself (via a prior passing proposal). None of these routes currently invalidates open
   proposals.

4. **No cross-proposal invalidation**: There is no mechanism that marks open proposals as
   `Cancelled` or `Stale` when the admin changes. The fix deliberately avoids adding such a
   mechanism in favour of a lazy check at execution time, keeping the change minimal.

---

## Correctness Properties

Property 1: Bug Condition — Stale Proposals Are Rejected

_For any_ `AdminProposal` in `Active` status where `isBugCondition` holds (i.e. the current
`DataKey::Admin` differs from `proposal.admin_at_creation`), the fixed `execute_admin_proposal`
SHALL return `Err(ContractError::StaleProposal)` and SHALL NOT modify `DataKey::Admin`.

**Validates: Requirements 2.1, 2.2, 2.3**

Property 2: Preservation — Non-Stale Proposals Behave Identically

_For any_ `AdminProposal` where `isBugCondition` does NOT hold (i.e. the current admin equals
`proposal.admin_at_creation`), the fixed `execute_admin_proposal` SHALL produce the same outcome
as the original function — executing passing proposals, defeating tied or losing proposals, and
applying all existing guards (`ProposalNotActive`, `ProposalVotingPeriodNotEnded`) unchanged.

**Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8**

---

## Fix Implementation

### Changes Required

Assuming the root cause analysis is correct, four files require changes.

---

**File**: `src/storage_types.rs`

**Change**: Add `admin_at_creation: Address` field to the `AdminProposal` struct.

```
// Before
pub struct AdminProposal {
    pub id:             u64,
    pub proposer:       Address,
    pub proposed_admin: Address,
    pub votes_for:      u64,
    pub votes_against:  u64,
    pub start_time:     u64,
    pub end_time:       u64,
    pub status:         ProposalStatus,
}

// After
pub struct AdminProposal {
    pub id:                 u64,
    pub proposer:           Address,
    pub proposed_admin:     Address,
    pub admin_at_creation:  Address,   // ← new field
    pub votes_for:          u64,
    pub votes_against:      u64,
    pub start_time:         u64,
    pub end_time:           u64,
    pub status:             ProposalStatus,
}
```

Adding a field to a `#[contracttype]` struct is a breaking storage change — existing on-chain
proposals will fail to deserialize. Because this is a testnet / pre-mainnet contract, no migration
is required; the spec assumes a clean deployment or a schema migration step is acceptable.

---

**File**: `src/errors.rs`

**Change 1**: Add `StaleProposal = 57` to `ContractError`.

**Change 2**: Bump `VARIANT_COUNT` from `56` to `57`.

**Change 3**: Append `ContractError::StaleProposal` to the end of `ALL_VARIANTS`.

```
// New variant
StaleProposal = 57,

// Updated constant
pub const VARIANT_COUNT: u32 = 57;
```

---

**File**: `src/governance.rs` — `create_admin_proposal`

**Change**: Capture `read_admin(&e)` into `admin_at_creation` when constructing `AdminProposal`.

```
// Before
let proposal = AdminProposal {
    id: proposal_id,
    proposer: proposer.clone(),
    proposed_admin: proposed_admin.clone(),
    votes_for: 0,
    votes_against: 0,
    start_time,
    end_time,
    status: ProposalStatus::Active,
};

// After
let admin_at_creation = read_admin(&e);

let proposal = AdminProposal {
    id: proposal_id,
    proposer: proposer.clone(),
    proposed_admin: proposed_admin.clone(),
    admin_at_creation,               // ← new field populated here
    votes_for: 0,
    votes_against: 0,
    start_time,
    end_time,
    status: ProposalStatus::Active,
};
```

---

**File**: `src/governance.rs` — `execute_admin_proposal`

**Change**: Insert a staleness check immediately after the `ProposalNotActive` and
`ProposalVotingPeriodNotEnded` guards, before the vote-tally branch.

```
// After the existing guards and before `if proposal.votes_for > proposal.votes_against`

let current_admin = read_admin(&e);
if current_admin != proposal.admin_at_creation {
    panic_with_error!(e, ContractError::StaleProposal);
}
```

The check must come **before** the vote-tally branch so that a stale proposal is rejected
regardless of whether it would have passed or been defeated. This keeps the contract's admin
state consistent: a defeated-but-stale proposal should not silently become a no-op; it should
fail loudly so callers know the proposal is no longer relevant.

---

## Testing Strategy

### Validation Approach

The testing strategy follows a two-phase approach: first write tests that demonstrate the bug on
unfixed code (exploratory / fix-checking tests will fail on the original), then verify the fix is
correct and no existing behaviour regresses.

---

### Exploratory Bug Condition Checking

**Goal**: Surface counterexamples that demonstrate the stale-proposal bug on the **unfixed** code.
Confirm the root cause hypothesis. If the tests do not fail on unfixed code as expected, revisit
the hypothesis.

**Test Plan**: Write tests that (a) create a proposal under admin A, (b) change the admin to
admin B via each of the three change routes, and (c) call `execute_admin_proposal` on the
now-stale proposal, asserting that the execution fails. Run these tests against the unfixed
codebase first to observe them pass (the bug is triggered and execution succeeds when it should
not), then verify they fail (the fix rejects execution).

**Test Cases**:

1. **`update_admin` route** — Create proposal under admin A; call `update_admin` to rotate to
   admin B; advance ledger past voting period; call `execute_admin_proposal`. Expected to **pass**
   (i.e. execution succeeds) on unfixed code, demonstrating the bug. (will fail on unfixed code
   to detect the staleness)

2. **Timelocked `SetAdmin` route** — Create proposal under admin A; enable timelock; schedule
   `SetAdmin(B)`; advance ledger past timelock delay; call `timelock_execute`; advance past
   proposal voting period; call `execute_admin_proposal`. Expected to pass on unfixed code.

3. **Concurrent-proposal race** — Create proposal 1 (proposing X) and proposal 2 (proposing Y)
   under admin A; cast passing votes on both; execute proposal 1 first (admin → X); execute
   proposal 2 second. Expected: proposal 2 execution silently overwrites X with Y on unfixed code.

4. **Out-of-band `update_admin` within the voting window** — Like case 1 but admin changes
   during the active voting period rather than after. Edge case: votes may have been cast under
   the old admin.

**Expected Counterexamples**:

- `execute_admin_proposal` succeeds and writes `proposed_admin` into `DataKey::Admin` even when
  the current admin no longer matches the proposal's creation-time admin.
- Possible root causes confirmed: `AdminProposal` lacks `admin_at_creation` field; execution
  branch has no staleness guard.

---

### Fix Checking

**Goal**: Verify that for all inputs where the bug condition holds, the fixed function returns
`StaleProposal` and leaves `DataKey::Admin` unchanged.

**Pseudocode:**

```
FOR ALL proposal WHERE isBugCondition(proposal, env) DO
  admin_before ← env.storage.instance.get(DataKey::Admin)
  result       ← execute_admin_proposal'(proposal.id)
  ASSERT result = Err(ContractError::StaleProposal)
  ASSERT env.storage.instance.get(DataKey::Admin) = admin_before
END FOR
```

**Specific fix-checking tests** (each runs on the fixed code and must pass):

1. `execute_stale_proposal_after_direct_update_admin_fails_with_stale_proposal` — verifies the
   `update_admin` route.
2. `execute_stale_proposal_after_timelock_set_admin_fails_with_stale_proposal` — verifies the
   timelocked route.
3. `execute_stale_proposal_after_prior_proposal_execution_fails_with_stale_proposal` — verifies
   the concurrent-proposal race.
4. `stale_proposal_does_not_modify_admin` — asserts that `DataKey::Admin` is unchanged after a
   `StaleProposal` rejection (property-based, many random admin addresses).

---

### Preservation Checking

**Goal**: Verify that for all inputs where the bug condition does NOT hold, the fixed function
produces the same result as the original function.

**Pseudocode:**

```
FOR ALL proposal WHERE NOT isBugCondition(proposal, env) DO
  ASSERT execute_admin_proposal_original(proposal) =
         execute_admin_proposal_fixed(proposal)
END FOR
```

**Testing Approach**: Property-based testing is recommended for preservation because:

- It generates many random `proposed_admin` addresses and admin states automatically.
- It catches edge cases (e.g. admin address equal to the proposed address) that manual tests miss.
- It provides a strong guarantee that all existing behaviour is preserved across the full
  non-buggy input domain.

**Test Plan**: Observe the existing behaviour of the five acceptance-criterion tests in
`src/governance_exec_test.rs` and the lifecycle test in `src/governance_test.rs` on the **unfixed**
code, then verify all those tests continue to pass after applying the fix.

**Preservation test cases**:

1. **Happy-path lifecycle unchanged** — The full create → vote → execute lifecycle with no admin
   change continues to set admin to `proposed_admin` and emit the `("gov", "executed")` event.

2. **Defeat path unchanged** — A proposal with `votes_against ≥ votes_for` continues to be
   marked `Defeated`; the `StaleProposal` check must not interfere with defeat.

3. **Pre-end-time guard unchanged** — Execution before `end_time` still fails with
   `ProposalVotingPeriodNotEnded` (staleness check comes after this guard, not before).

4. **`ProposalNotActive` guard unchanged** — Re-executing a defeated, executed, or cancelled
   proposal still fails with `ProposalNotActive`.

5. **Timelock routing unchanged** — When the timelock is enabled and the proposal passes (no
   admin drift), execution still schedules `TimelockAction::SetAdmin` instead of writing
   `DataKey::Admin` directly.

6. **Cancellation unchanged** — `cancel_admin_proposal` by proposer or current admin continues
   to work; the new `admin_at_creation` field does not affect cancellation logic.

---

### Unit Tests

- Test that `create_admin_proposal` stores the current admin address in `admin_at_creation`.
- Test `execute_admin_proposal` with each admin-change route (direct, timelock, prior proposal)
  to verify `StaleProposal` is returned and `DataKey::Admin` is not modified.
- Test that `execute_admin_proposal` still succeeds when admin has not changed (regression).
- Test that `StaleProposal` has the expected error code (57) to avoid accidental code-collision
  with existing errors.

### Property-Based Tests

- **Fix property** (in `src/prop_test.rs`): for any arbitrary `Address` used as admin, create a
  proposal, rotate admin to a different arbitrary `Address`, advance past voting, call
  `execute_admin_proposal`, and assert the result is `Err(StaleProposal)` and `DataKey::Admin`
  is unchanged. Generate many (address, new\_admin) pairs.
- **Preservation property**: for any arbitrary `proposed_admin` address and any sequence of votes,
  when no admin rotation occurs between creation and execution, the proposal outcome (Executed vs.
  Defeated) and the resulting admin state are identical to the pre-fix behaviour.
- **No false positives**: for any proposal where `admin_at_creation == current_admin`, confirm
  that the `StaleProposal` branch is never taken, regardless of vote tallies.

### Integration Tests

- Full end-to-end: create proposal under admin A, execute a timelocked `SetAdmin(B)`, attempt to
  execute the stale proposal, assert `StaleProposal`, verify admin is still B, then create a new
  proposal under admin B, vote it through, and verify admin changes to the new `proposed_admin`.
- Concurrent-proposal sequence: proposal 1 executes successfully (admin → X), proposal 2 (created
  under original admin A) correctly returns `StaleProposal` when executed.
- Verify the `docs/errors.md` narrative is updated to document the new `StaleProposal` error code (57).
