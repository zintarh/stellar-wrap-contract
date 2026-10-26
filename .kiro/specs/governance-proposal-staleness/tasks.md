# Implementation Plan

- [x] 1. Write bug condition exploration test
  - **Property 1: Bug Condition** - Stale Proposal Executes Silently
  - **CRITICAL**: This test MUST FAIL on unfixed code — failure confirms the bug exists
  - **DO NOT attempt to fix the test or the code when it fails**
  - **NOTE**: This test encodes the expected behavior — it will validate the fix when it passes after implementation
  - **GOAL**: Surface counterexamples that demonstrate that `execute_admin_proposal` succeeds (writes `proposed_admin`) even when `current_admin ≠ proposal.admin_at_creation`
  - **Scoped PBT Approach**: For a deterministic core case, scope the property to the `update_admin` route (admin A → admin B before execution); additionally cover the timelocked `SetAdmin` route and the concurrent-proposal race as parameterised sub-cases
  - Create a new test file `src/governance_staleness_test.rs` (or add to `src/governance_test.rs`)
  - Write a property-based test covering all three admin-change routes:
    - **Route 1** (direct): create proposal under admin A; call `update_admin` to rotate to admin B; advance ledger past `end_time`; call `execute_admin_proposal` — assert `Err(ContractError::StaleProposal)` AND admin is still B
    - **Route 2** (timelock): create proposal under admin A; enable timelock; schedule `SetAdmin(B)`; advance past timelock delay; call `timelock_execute`; advance past proposal `end_time`; call `execute_admin_proposal` — assert `Err(ContractError::StaleProposal)` AND admin is still B
    - **Route 3** (concurrent proposals): create proposals P1 and P2 under admin A (both pass votes); execute P1 first (admin → X); call `execute_admin_proposal` for P2 — assert `Err(ContractError::StaleProposal)` AND admin is still X
  - For PBT: generate arbitrary `Address` pairs `(admin_a, admin_b)` where `admin_a ≠ admin_b`; for each pair run Route 1; assert the error code is `57` (`StaleProposal`) and `get_admin()` equals `admin_b`
  - Run test on UNFIXED code
  - **EXPECTED OUTCOME**: Test FAILS on unfixed code (execution succeeds and overwrites admin — this proves the bug exists)
  - Document counterexamples found, e.g. `"execute_admin_proposal(proposal_id) succeeds and sets admin to proposed_admin even though current admin is admin_b ≠ admin_a (admin_at_creation)"`
  - Mark task complete when test is written, run, and failure is documented
  - _Requirements: 1.1, 1.2, 1.3, 1.4_

- [x] 2. Write preservation property tests (BEFORE implementing fix)
  - **Property 2: Preservation** - Non-Stale Proposal Lifecycle Unchanged
  - **IMPORTANT**: Follow observation-first methodology
  - Observe behavior on UNFIXED code for non-buggy inputs (cases where `admin_at_creation == current_admin`, i.e. `isBugCondition` returns `false`)
  - **Observations to record on unfixed code**:
    - `execute_admin_proposal` on a passing proposal (no admin change) → status `Executed`, admin updated to `proposed_admin`, `("gov", "executed")` event emitted
    - `execute_admin_proposal` on a defeated proposal (0 for, 1 against) → status `Defeated`, admin unchanged
    - `execute_admin_proposal` before `end_time` → panics with `Error(Contract, #28)` (`ProposalVotingPeriodNotEnded`)
    - Re-executing an `Executed` proposal → panics with `Error(Contract, #26)` (`ProposalNotActive`)
    - Re-executing a `Defeated` proposal → panics with `Error(Contract, #26)` (`ProposalNotActive`)
    - `execute_admin_proposal` on a `Cancelled` proposal → panics with `Error(Contract, #26)` (`ProposalNotActive`)
    - `execute_admin_proposal` with timelock enabled and no admin drift → schedules `TimelockAction::SetAdmin`, does NOT write `DataKey::Admin` directly
    - Tie vote (votes_for == votes_against) → status `Defeated`, admin unchanged
  - Write property-based tests capturing observed behavior patterns from Preservation Requirements (design §"Preservation Requirements"):
    - **Preservation PBT 1**: for any arbitrary `proposed_admin` address and passing vote configuration (`votes_for > votes_against`), with no admin change between creation and execution, outcome is always `Executed` and admin equals `proposed_admin`
    - **Preservation PBT 2**: for any arbitrary vote configuration where `votes_against ≥ votes_for`, outcome is always `Defeated` and admin is unchanged
    - **Preservation PBT 3** (no false positives): for any proposal where `admin_at_creation == current_admin`, the `StaleProposal` branch (`#57`) is never taken regardless of vote tallies
  - Verify all of the following existing tests still pass on UNFIXED code (baseline): `src/governance_exec_test.rs` (5 acceptance-criterion tests + 2 supplementary) and `src/governance_test.rs` (`test_governance_lifecycle`)
  - Run preservation tests on UNFIXED code
  - **EXPECTED OUTCOME**: All preservation tests PASS on unfixed code (confirms the baseline behavior to preserve)
  - Mark task complete when tests are written, run, and passing on unfixed code
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8_

- [x] 3. Fix for governance proposal staleness — stale proposals execute silently when admin changes

  - [x] 3.1 Add `admin_at_creation: Address` field to `AdminProposal` struct in `src/storage_types.rs`
    - Locate the `AdminProposal` `#[contracttype]` struct
    - Insert `pub admin_at_creation: Address,` after `proposed_admin` and before `votes_for`
    - This field is immutable after creation and records `DataKey::Admin` at proposal-creation time
    - Note: adding a field to `#[contracttype]` is a breaking storage change; a clean deployment or migration is required
    - _Bug_Condition: `isBugCondition(proposal, env)` where `env.storage.instance.get(DataKey::Admin) ≠ proposal.admin_at_creation`_
    - _Expected_Behavior: `admin_at_creation` holds the `DataKey::Admin` value snapshotted when `create_admin_proposal` was called_
    - _Preservation: All existing fields and their semantics are unchanged; field order in the struct must be preserved except for insertion of the new field_
    - _Requirements: 2.4_

  - [x] 3.2 Add `StaleProposal = 57` variant to `ContractError` in `src/errors.rs`
    - Append `StaleProposal = 57,` after `TimelockOperationNotExpired = 56`
    - Bump `VARIANT_COUNT` from `56` to `57`
    - Append `ContractError::StaleProposal` to the end of the `ALL_VARIANTS` array
    - Verify discriminant 57 does not collide with any existing variant
    - _Bug_Condition: execution branch in `execute_admin_proposal` has no error variant to return when `current_admin ≠ proposal.admin_at_creation`_
    - _Expected_Behavior: `ContractError::StaleProposal` (code 57) is the rejection returned by `execute_admin_proposal` for any proposal satisfying `isBugCondition`_
    - _Preservation: All existing variants and their discriminants (1–56) are unchanged_
    - _Requirements: 2.1, 2.2, 2.3_

  - [x] 3.3 Snapshot `admin_at_creation` in `create_admin_proposal` in `src/governance.rs`
    - Before constructing `AdminProposal`, call `read_admin(&e)` and bind the result to `admin_at_creation`
    - Add `admin_at_creation` to the `AdminProposal { ... }` struct literal
    - The snapshot must happen after `proposer.require_auth()` and before `e.storage().persistent().set(...)`
    - _Bug_Condition: `AdminProposal` currently stores no creation-time admin snapshot; `execute_admin_proposal` therefore has no reference point for staleness detection_
    - _Expected_Behavior: every newly created `AdminProposal` has `admin_at_creation` equal to `read_admin(&e)` at creation time — `isBugCondition` can now be evaluated at execution time_
    - _Preservation: All other fields of `AdminProposal` are populated identically to before; proposal count, event emission, and authorization checks are unchanged_
    - _Requirements: 2.4_

  - [x] 3.4 Insert staleness guard in `execute_admin_proposal` in `src/governance.rs`
    - After the `ProposalVotingPeriodNotEnded` guard (`if now <= proposal.end_time`) and before the vote-tally branch (`if proposal.votes_for > proposal.votes_against`), insert:
      ```rust
      let current_admin = read_admin(&e);
      if current_admin != proposal.admin_at_creation {
          panic_with_error!(e, ContractError::StaleProposal);
      }
      ```
    - The guard must be placed **before** the vote-tally branch so stale proposals are rejected regardless of whether they would have passed or been defeated
    - Do not modify any other guard or branch
    - _Bug_Condition: `isBugCondition(proposal, env)` ↔ `read_admin(&e) ≠ proposal.admin_at_creation`_
    - _Expected_Behavior: `execute_admin_proposal'` returns `Err(ContractError::StaleProposal)` for all proposals satisfying `isBugCondition`; `DataKey::Admin` is NOT modified_
    - _Preservation: For proposals where `isBugCondition` is `false`, all existing guards (`ProposalNotActive`, `ProposalVotingPeriodNotEnded`) and the vote-tally branch (`Executed` vs `Defeated`) continue to operate identically — see Preservation Requirements in design_
    - _Requirements: 2.1, 2.2, 2.3, 3.1, 3.2, 3.5, 3.6_

  - [x] 3.5 Verify bug condition exploration test now passes
    - **Property 1: Expected Behavior** - Stale Proposal Returns `StaleProposal` Error
    - **IMPORTANT**: Re-run the SAME test written in task 1 — do NOT write a new test
    - The test from task 1 encodes the expected behavior (`execute_admin_proposal` returns `Err(ContractError::StaleProposal)` and admin is unchanged for all buggy inputs)
    - Run the bug condition exploration test against the FIXED code
    - **EXPECTED OUTCOME**: Test PASSES (confirms the bug is fixed for all three admin-change routes and arbitrary address pairs)
    - Verify error code is `57` in the panic message (`Error(Contract, #57)`)
    - Verify `get_admin()` equals the post-change admin address (not `proposed_admin`) after each rejected execution
    - _Requirements: 2.1, 2.2, 2.3, 2.4_

  - [x] 3.6 Verify preservation tests still pass
    - **Property 2: Preservation** - Non-Stale Proposal Lifecycle Unchanged
    - **IMPORTANT**: Re-run the SAME tests written in task 2 — do NOT write new tests
    - Run all preservation property tests from task 2 against the FIXED code
    - Run the full existing test suite: `src/governance_exec_test.rs` and `src/governance_test.rs`
    - **EXPECTED OUTCOME**: All tests PASS (confirms no regressions in the non-stale path)
    - Confirm the following specific behaviors are intact:
      - Passing proposal with no admin drift → `Executed`, admin updated to `proposed_admin`
      - Defeated proposal → `Defeated`, admin unchanged
      - Pre-end-time execution → `Error(Contract, #28)` (`ProposalVotingPeriodNotEnded`)
      - Re-execution of non-active proposal → `Error(Contract, #26)` (`ProposalNotActive`)
      - Timelock-enabled passing proposal (no drift) → `SetAdmin` scheduled, `DataKey::Admin` not written directly
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 3.8_

- [x] 4. Checkpoint — Ensure all tests pass
  - Run the full test suite (`cargo test` in the workspace root)
  - Confirm zero failures and zero unexpected passes
  - Verify `ContractError::VARIANT_COUNT` is `57` and `ALL_VARIANTS` has exactly 57 entries
  - Verify `StaleProposal` error code 57 is documented: update `docs/errors.md` to add an entry for `StaleProposal = 57` with a description of when it is returned
  - Ask the user if any questions arise before closing the fix
