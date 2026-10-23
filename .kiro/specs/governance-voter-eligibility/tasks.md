# Implementation Plan

- [ ] 1. Write bug condition exploration test
  - **Property 1: Bug Condition** — Ineligible voter can cast a vote
  - **CRITICAL**: This test MUST FAIL on unfixed code — failure confirms the bug exists
  - **DO NOT fix the production code while writing this test**
  - Create `src/governance_voter_eligibility_test.rs`
  - Write tests covering:
    - `test_non_staker_cannot_vote`: address with no stake calls `vote_admin_proposal` — assert `VoterNotEligible` (#58) and vote counts unchanged
    - `test_unstaking_address_cannot_vote`: address that called `unstake()` (unstaking_at ≠ 0) calls `vote_admin_proposal` — assert `VoterNotEligible` (#58)
    - PBT: for arbitrary addresses with no stake, all calls return `VoterNotEligible`
  - Register module in `src/lib.rs` as `mod governance_voter_eligibility_test;`
  - Run tests on UNFIXED code — **EXPECTED OUTCOME**: tests FAIL (ineligible votes are accepted)
  - Document counterexamples: e.g. `"vote_admin_proposal accepted a vote from an address with no stake"`
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5_

- [ ] 2. Write preservation property tests (BEFORE implementing fix)
  - **Property 2: Preservation** — Eligible voter behavior is unchanged
  - Create preservation tests in `src/governance_voter_eligibility_test.rs`:
    - `test_active_staker_can_vote`: staked address votes → recorded, event emitted
    - `test_double_vote_still_rejected`: eligible voter votes twice → `ProposalAlreadyVoted` (#27)
    - `test_vote_after_window_still_rejected`: eligible voter votes after end_time → `ProposalVotingPeriodEnded` (#29)
    - `test_vote_on_cancelled_proposal_still_rejected`: eligible voter votes on cancelled proposal → `ProposalNotActive` (#26)
  - Run tests on UNFIXED code — **EXPECTED OUTCOME**: all preservation tests PASS
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

- [ ] 3. Fix for governance voter eligibility — ineligible addresses can vote

  - [ ] 3.1 Add `VoterNotEligible = 58` variant to `ContractError` in `src/errors.rs`
    - Append `VoterNotEligible = 58,` after `StaleProposal = 57`
    - Add doc comment: `/// Caller is not eligible to vote: no active stake record (issue #865).`
    - Bump `VARIANT_COUNT` from `57` to `58`
    - Append `ContractError::VoterNotEligible` to end of `ALL_VARIANTS`
    - _Requirements: 2.1_

  - [ ] 3.2 Insert eligibility check in `vote_admin_proposal` in `src/governance.rs`
    - After `voter.require_auth()` and before the proposal fetch, insert:
      ```rust
      let stake_record: Option<crate::storage_types::StakeRecord> = e
          .storage()
          .persistent()
          .get(&DataKey::Stake(voter.clone()));
      match stake_record {
          Some(r) if r.unstaking_at == 0 => {}
          _ => panic_with_error!(e, ContractError::VoterNotEligible),
      }
      ```
    - No other changes to `vote_admin_proposal`
    - _Bug_Condition: `isBugCondition(voter)` ↔ no active StakeRecord_
    - _Expected_Behavior: returns `Err(VoterNotEligible)` for ineligible callers; vote counts unchanged_
    - _Preservation: all existing guards and vote recording remain identical for eligible voters_
    - _Requirements: 2.1, 2.2, 2.5_

  - [ ] 3.3 Document eligibility model in `docs/authority-model.md`
    - Add a "Governance Voter Eligibility" section stating:
      - Eligibility criterion: active staker (`StakeRecord` present, `unstaking_at == 0`)
      - Vote weight: flat (1 vote per eligible address)
      - Snapshot point: at vote-cast time
      - Sybil consequence: attacker must lock capital ≥ `min_stake` per vote
    - _Requirements: 2.4, 2.6_

  - [ ] 3.4 Verify bug condition exploration test now passes
    - Re-run tests from Task 1 on FIXED code
    - **EXPECTED OUTCOME**: all three tests PASS (`VoterNotEligible` is returned)
    - _Requirements: 2.1, 2.2_

  - [ ] 3.5 Verify preservation tests still pass
    - Re-run tests from Task 2 on FIXED code
    - Run existing `src/governance_exec_test.rs` and `src/governance_test.rs`
    - **EXPECTED OUTCOME**: all tests PASS (eligible voter behavior unchanged)
    - Note: existing tests use mock addresses without stakes — they will need staking setup added
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

- [ ] 4. Update existing governance tests to stake voters
  - All existing tests in `src/governance_exec_test.rs` and `src/governance_test.rs` that call
    `vote_admin_proposal` must now set up a `StakeRecord` for the voter first
  - Add a `stake_voter(client, voter)` helper to the test setup
  - Update every `client.vote_admin_proposal(...)` call site that uses an unstaked address
  - Run `cargo test` — all governance tests must pass
  - _Requirements: 3.1_

- [ ] 5. Document `VoterNotEligible = 58` in `docs/errors.md`
  - Append an entry for error code 58 explaining when it is returned and how to resolve it
  - _Requirements: 2.6_

- [ ] 6. Checkpoint — Ensure all tests pass
  - Run `cargo test` from workspace root
  - Confirm zero failures
  - Verify `VARIANT_COUNT == 58` and `ALL_VARIANTS` has 58 entries
