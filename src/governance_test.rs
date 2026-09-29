#![cfg(test)]
extern crate std;

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::{AdminProposal, ProposalStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, IntoVal, Symbol, TryIntoVal,
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
    let events = crate::test_utils::decode_events(&env);
    let (topics, data) = events.last().unwrap();
    let topic0: Symbol = topics[0].try_into_val(&env).unwrap();
    let topic1: Symbol = topics[1].try_into_val(&env).unwrap();
    assert_eq!(topic0, symbol_short!("gov"));
    assert_eq!(topic1, symbol_short!("propose"));
    let (p_id, p_proposer, p_admin): (u64, Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(p_id, proposal_id);
    assert_eq!(p_proposer, proposer);
    assert_eq!(p_admin, proposed_admin);

    // 2. Voting
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);

    client.vote_admin_proposal(&voter1, &proposal_id, &true);
    let events_vote1 = crate::test_utils::decode_events(&env);
    let (topics_vote1, data_vote1) = events_vote1.last().unwrap();
    let topic0: Symbol = topics_vote1[0].try_into_val(&env).unwrap();
    let topic1: Symbol = topics_vote1[1].try_into_val(&env).unwrap();
    assert_eq!(topic0, symbol_short!("gov"));
    assert_eq!(topic1, symbol_short!("vote"));
    let (p_id, p_voter, p_support): (u64, Address, bool) = data_vote1.try_into_val(&env).unwrap();
    assert_eq!(p_id, proposal_id);
    assert_eq!(p_voter, voter1);
    assert_eq!(p_support, true);

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
    let events_exec = crate::test_utils::decode_events(&env);
    let (topics_exec, data_exec) = events_exec.last().unwrap();
    let topic0: Symbol = topics_exec[0].try_into_val(&env).unwrap();
    let topic1: Symbol = topics_exec[1].try_into_val(&env).unwrap();
    assert_eq!(topic0, symbol_short!("gov"));
    assert_eq!(topic1, symbol_short!("executed"));
    let (p_id, p_admin): (u64, Address) = data_exec.try_into_val(&env).unwrap();
    assert_eq!(p_id, proposal_id);
    assert_eq!(p_admin, proposed_admin);

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
