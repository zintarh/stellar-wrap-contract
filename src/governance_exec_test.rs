#![cfg(test)]
//! Governance execution guard tests — issue #686.
//!
//! Covers every branch of `execute_admin_proposal`:
//!   1. Pre-end-time rejection  (ProposalVotingPeriodNotEnded)
//!   2. Defeat path             (votes_against ≥ votes_for)
//!   3. Defeat persistence      (status persisted after defeat)
//!   4. Tie handling            (votes_for == votes_against ⇒ Defeated)
//!   5. Double-execution guard  (ProposalNotActive on re-execute)

extern crate std;

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Minimal setup: deploy contract, initialise with admin, mock auths.
/// Returns (env, client, contract_id, admin, proposer, proposed_admin).
fn setup() -> (
    Env,
    StellarWrapContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let proposer = Address::generate(&env);
    let proposed_admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    env.mock_all_auths();
    client.initialize(&admin, &pubkey);

    (env, client, contract_id, admin, proposer, proposed_admin)
}

/// Create a proposal with the given `duration_seconds` and return its id.
fn create_proposal(
    client: &StellarWrapContractClient,
    proposer: &Address,
    proposed_admin: &Address,
    duration_seconds: u64,
) -> u64 {
    client.create_admin_proposal(proposer, proposed_admin, &duration_seconds)
}

// ── Acceptance criterion 1 ──────────────────────────────────────────────────
// Executing before end_time fails with ProposalVotingPeriodNotEnded.

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn execute_before_end_time_fails_with_voting_period_not_ended() {
    let (_env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    // time = 0, end_time = 100 → now <= end_time → panic #28
    client.execute_admin_proposal(&proposal_id);
}

// ── Acceptance criterion 2 ──────────────────────────────────────────────────
// A proposal with votes_against ≥ votes_for fails with ProposalDefeated.
// Note: `execute_admin_proposal` does NOT return ProposalDefeated — it marks
// the proposal Defeated and returns normally.  We verify the vote tally leads
// to the Defeated outcome by inspecting persisted state.

#[test]
fn proposal_with_more_against_votes_is_defeated() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    // 0 for, 1 against → votes_against > votes_for → Defeated
    client.vote_admin_proposal(&proposer, &proposal_id, &false);

    env.ledger().with_mut(|l| l.timestamp += 101);
    client.execute_admin_proposal(&proposal_id);

    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Defeated);
}

// ── Acceptance criterion 3 ──────────────────────────────────────────────────
// After a defeat, the persisted status is asserted.
// Documents the rollback-bug discussion: the Defeated status IS persisted.

#[test]
fn defeated_status_persists_after_execution() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    // 1 for (proposer votes yes), 2 against (two extra voters vote no) → Defeated
    let voter_b = Address::generate(&env);
    let voter_c = Address::generate(&env);
    client.vote_admin_proposal(&proposer, &proposal_id, &true);
    client.vote_admin_proposal(&voter_b, &proposal_id, &false);
    client.vote_admin_proposal(&voter_c, &proposal_id, &false);

    env.ledger().with_mut(|l| l.timestamp += 101);
    client.execute_admin_proposal(&proposal_id);

    // Verify Defeated status was persisted
    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(
        proposal.status,
        ProposalStatus::Defeated,
        "Defeated status must be persisted after execution"
    );

    // Re-executing the same proposal must fail with ProposalNotActive (#26)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.execute_admin_proposal(&proposal_id);
    }));
    assert!(
        result.is_err(),
        "re-executing a defeated proposal must fail"
    );
    let err_msg = result
        .unwrap_err()
        .downcast_ref::<std::string::String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        err_msg.contains("Error(Contract, #26)"),
        "expected ProposalNotActive (#26), got: {err_msg}"
    );
}

// ── Acceptance criterion 4 ──────────────────────────────────────────────────
// A tie (votes_for == votes_against) is defeated, not executed.

#[test]
fn tie_is_defeated_not_executed() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    let voter = Address::generate(&env);
    // 1 for (proposer), 1 against (voter) → tie → Defeated
    client.vote_admin_proposal(&proposer, &proposal_id, &true);
    client.vote_admin_proposal(&voter, &proposal_id, &false);

    env.ledger().with_mut(|l| l.timestamp += 101);
    client.execute_admin_proposal(&proposal_id);

    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(
        proposal.status,
        ProposalStatus::Defeated,
        "a tie must result in Defeated, not Executed"
    );

    // Admin must not have changed
    let admin_key = client.get_admin().unwrap();
    assert_ne!(
        admin_key, proposed_admin,
        "admin must not change on a defeated proposal"
    );
}

// ── Acceptance criterion 5 ──────────────────────────────────────────────────
// Executing an already-executed proposal fails with ProposalNotActive.

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn execute_already_executed_proposal_fails_with_not_active() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    // 1 for (proposer), 0 against → Executed
    client.vote_admin_proposal(&proposer, &proposal_id, &true);

    env.ledger().with_mut(|l| l.timestamp += 101);

    // First execution succeeds
    client.execute_admin_proposal(&proposal_id);

    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);

    // Second execution must fail with ProposalNotActive (#26)
    client.execute_admin_proposal(&proposal_id);
}

// ── Supplementary: cancelled proposal cannot be executed ─────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn execute_cancelled_proposal_fails_with_not_active() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    client.cancel_admin_proposal(&proposer, &proposal_id);

    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Cancelled);

    // Cannot execute after cancellation
    env.ledger().with_mut(|l| l.timestamp += 101);
    client.execute_admin_proposal(&proposal_id);
}

// ── Supplementary: passing execution sets status to Executed ────────────────

#[test]
fn passing_execution_sets_status_to_executed() {
    let (env, client, _contract_id, _admin, proposer, proposed_admin) = setup();
    let proposal_id = create_proposal(&client, &proposer, &proposed_admin, 100);

    // 1 for (proposer), 0 against → Executed
    client.vote_admin_proposal(&proposer, &proposal_id, &true);

    env.ledger().with_mut(|l| l.timestamp += 101);
    client.execute_admin_proposal(&proposal_id);

    let proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);

    // Admin should have been updated (no timelock)
    assert_eq!(client.get_admin().unwrap(), proposed_admin);


}
