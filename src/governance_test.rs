#![cfg(test)]
extern crate std;

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::{AdminProposal, ProposalStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, IntoVal, TryIntoVal,
};

fn setup_env() -> (Env, StellarWrapContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[1; 32]);
    client.initialize(&admin, &admin_pubkey);

    (env, client, admin)
}

#[test]
fn test_governance_lifecycle() {
    let (env, client, original_admin) = setup_env();

    // Setup time
    env.ledger().with_mut(|li| {
        li.timestamp = 1000;
    });

    let proposer = Address::generate(&env);
    let proposed_admin = Address::generate(&env);
    let duration: u64 = crate::governance::MIN_PROPOSAL_DURATION;

    // 1. Create a proposal
    let proposal_id = client.create_admin_proposal(&proposer, &proposed_admin, &duration);
    assert_eq!(proposal_id, 1);
    // Events only cover the latest invocation, so capture them right away.
    let propose_events = crate::test_utils::contract_events(&env);

    // Verify stored proposal
    let stored_proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(stored_proposal.id, proposal_id);
    assert_eq!(stored_proposal.proposer, proposer);
    assert_eq!(stored_proposal.proposed_admin, proposed_admin);
    assert_eq!(stored_proposal.votes_for, 0);
    assert_eq!(stored_proposal.votes_against, 0);
    assert_eq!(stored_proposal.start_time, 1000);
    assert_eq!(stored_proposal.end_time, 1000 + crate::governance::MIN_PROPOSAL_DURATION);
    assert_eq!(stored_proposal.status, ProposalStatus::Active);

    // Verify 'propose' event
    let event = propose_events.last().unwrap();
    assert_eq!(
        event.1,
        (symbol_short!("gov"), symbol_short!("propose")).into_val(&env)
    );
    let data: (u64, Address, Address) = event.2.try_into_val(&env).unwrap();
    assert_eq!(data, (proposal_id, proposer.clone(), proposed_admin.clone()));

    // 2. Voting
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);

    client.vote_admin_proposal(&voter1, &proposal_id, &true);
    let event_vote1 = crate::test_utils::contract_events(&env).last().unwrap();
    assert_eq!(
        event_vote1.1,
        (symbol_short!("gov"), symbol_short!("vote")).into_val(&env)
    );
    let data: (u64, Address, bool) = event_vote1.2.try_into_val(&env).unwrap();
    assert_eq!(data, (proposal_id, voter1.clone(), true));

    client.vote_admin_proposal(&voter2, &proposal_id, &true);
    client.vote_admin_proposal(&voter3, &proposal_id, &false);

    let current_proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(current_proposal.votes_for, 2);
    assert_eq!(current_proposal.votes_against, 1);

    // 3. Fast forward past end_time and execute
    env.ledger().with_mut(|li| {
        li.timestamp = 1000 + crate::governance::MIN_PROPOSAL_DURATION + 1;
    });

    client.execute_admin_proposal(&proposal_id);

    // Verify 'executed' event
    let event_exec = crate::test_utils::contract_events(&env).last().unwrap();
    assert_eq!(
        event_exec.1,
        (symbol_short!("gov"), symbol_short!("executed")).into_val(&env)
    );
    let data: (u64, Address) = event_exec.2.try_into_val(&env).unwrap();
    assert_eq!(data, (proposal_id, proposed_admin.clone()));

    let executed_proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(executed_proposal.status, ProposalStatus::Executed);

    // 4. Verify admin is updated and can perform admin action
    let new_admin = client.get_admin().unwrap();
    assert_eq!(new_admin, proposed_admin);

    // Perform an admin-only action
    // set_pause is admin-only, requires auth from the new admin.
    client.pause();
    assert!(client.is_paused());
}
