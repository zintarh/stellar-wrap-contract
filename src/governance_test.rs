#![cfg(test)]
extern crate std;

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::{AdminProposal, ProposalStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, IntoVal,
};

fn setup_env() -> (Env, StellarWrapContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let admin_pubkey = BytesN::from_array(&env, &[0; 32]);
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
    let duration: u64 = 600;

    // 1. Create a proposal
    let proposal_id = client.create_admin_proposal(&proposer, &proposed_admin, &duration);
    assert_eq!(proposal_id, 1);

    // Verify stored proposal
    let stored_proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(stored_proposal.id, proposal_id);
    assert_eq!(stored_proposal.proposer, proposer);
    assert_eq!(stored_proposal.proposed_admin, proposed_admin);
    assert_eq!(stored_proposal.votes_for, 0);
    assert_eq!(stored_proposal.votes_against, 0);
    assert_eq!(stored_proposal.start_time, 1000);
    assert_eq!(stored_proposal.end_time, 1600);
    assert_eq!(stored_proposal.status, ProposalStatus::Active);

    // Verify 'propose' event
    let events = env.events().all();
    let event = events.last().unwrap();
    assert_eq!(
        event.topics,
        (symbol_short!("gov"), symbol_short!("propose")).into_val(&env)
    );
    assert_eq!(
        event.data,
        (proposal_id, proposer.clone(), proposed_admin.clone()).into_val(&env)
    );

    // 2. Voting
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);

    client.vote_admin_proposal(&voter1, &proposal_id, &true);
    let event_vote1 = env.events().all().last().unwrap();
    assert_eq!(
        event_vote1.topics,
        (symbol_short!("gov"), symbol_short!("vote")).into_val(&env)
    );
    assert_eq!(
        event_vote1.data,
        (proposal_id, voter1.clone(), true).into_val(&env)
    );

    client.vote_admin_proposal(&voter2, &proposal_id, &true);
    client.vote_admin_proposal(&voter3, &proposal_id, &false);

    let current_proposal = client.get_admin_proposal(&proposal_id).unwrap();
    assert_eq!(current_proposal.votes_for, 2);
    assert_eq!(current_proposal.votes_against, 1);

    // 3. Fast forward past end_time and execute
    env.ledger().with_mut(|li| {
        li.timestamp = 1601;
    });

    client.execute_admin_proposal(&proposal_id);

    // Verify 'executed' event
    let event_exec = env.events().all().last().unwrap();
    assert_eq!(
        event_exec.topics,
        (symbol_short!("gov"), symbol_short!("executed")).into_val(&env)
    );
    assert_eq!(
        event_exec.data,
        (proposal_id, proposed_admin.clone()).into_val(&env)
    );

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
