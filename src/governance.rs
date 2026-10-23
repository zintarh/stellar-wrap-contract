use soroban_sdk::{panic_with_error, symbol_short, Address, Env};

use crate::{
    admin::read_admin, storage_types::TimelockAction, AdminProposal, ContractError, DataKey,
    ProposalStatus,
};

/// Typed accessor for governance storage. All governance storage access goes through
/// this module so that tier choice and TTL policy live in one place.
mod store {
    use soroban_sdk::{Address, Env};

    use crate::{AdminProposal, DataKey};

    pub(super) fn proposal_count(e: &Env) -> u64 {
        e.storage()
            .instance()
            .get(&DataKey::AdminProposalCount)
            .unwrap_or(0)
    }

    pub(super) fn set_proposal_count(e: &Env, count: u64) {
        e.storage()
            .instance()
            .set(&DataKey::AdminProposalCount, &count);
    }

    pub(super) fn proposal(e: &Env, id: u64) -> Option<AdminProposal> {
        e.storage()
            .persistent()
            .get(&DataKey::AdminProposal(id))
    }

    pub(super) fn set_proposal(e: &Env, id: u64, proposal: &AdminProposal) {
        e.storage()
            .persistent()
            .set(&DataKey::AdminProposal(id), proposal);
    }

    pub(super) fn has_vote(e: &Env, id: u64, voter: &Address) -> bool {
        e.storage()
            .persistent()
            .has(&DataKey::AdminProposalVote(id, voter.clone()))
    }

    pub(super) fn vote(e: &Env, id: u64, voter: &Address) -> Option<bool> {
        e.storage()
            .persistent()
            .get(&DataKey::AdminProposalVote(id, voter.clone()))
    }

    pub(super) fn set_vote(e: &Env, id: u64, voter: &Address, support: bool) {
        e.storage()
            .persistent()
            .set(&DataKey::AdminProposalVote(id, voter.clone()), &support);
    }

    pub(super) fn set_admin(e: &Env, admin: &Address) {
        e.storage().instance().set(&DataKey::Admin, admin);
    }

    pub(super) fn clear_pending_admin(e: &Env) {
        e.storage().instance().remove(&DataKey::PendingAdmin);
    }
}

/// Minimum duration for an admin proposal (1 hour).
pub const MIN_PROPOSAL_DURATION: u64 = 60 * 60;
/// Maximum duration for an admin proposal (30 days).
pub const MAX_PROPOSAL_DURATION: u64 = 30 * 24 * 60 * 60;

/// Create a new proposal to update the contract admin.
/// Returns the generated proposal ID.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn create_admin_proposal(
    e: Env,
    proposer: Address,
    proposed_admin: Address,
    duration_seconds: u64,
) -> u64 {
    proposer.require_auth();

    if duration_seconds < MIN_PROPOSAL_DURATION || duration_seconds > MAX_PROPOSAL_DURATION {
        panic_with_error!(e, ContractError::InvalidProposalDuration);
    }
    
    let count = store::proposal_count(&e);
    let proposal_id = count + 1;

    let start_time = e.ledger().timestamp();
    let end_time = start_time
        .checked_add(duration_seconds)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::InvalidProposalDuration));

    let admin_at_creation = read_admin(&e);

    let proposal = AdminProposal {
        id: proposal_id,
        proposer: proposer.clone(),
        proposed_admin: proposed_admin.clone(),
        admin_at_creation,
        votes_for: 0,
        votes_against: 0,
        start_time,
        end_time,
        status: ProposalStatus::Active,
    };

    store::set_proposal_count(&e, proposal_id);
    store::set_proposal(&e, proposal_id, &proposal);

    e.events().publish(
        (symbol_short!("gov"), symbol_short!("propose")),
        (proposal_id, proposer, proposed_admin),
    );

    proposal_id

}

/// Cast a vote on an active governance proposal.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn vote_admin_proposal(e: Env, voter: Address, proposal_id: u64, support: bool) {
    voter.require_auth();

    let mut proposal: AdminProposal = store::proposal(&e, proposal_id)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

    if proposal.status != ProposalStatus::Active {
        panic_with_error!(e, ContractError::ProposalNotActive);
    }

    let now = e.ledger().timestamp();
    if now > proposal.end_time {
        panic_with_error!(e, ContractError::ProposalVotingPeriodEnded);
    }

    if store::has_vote(&e, proposal_id, &voter) {
        panic_with_error!(e, ContractError::ProposalAlreadyVoted);
    }

    store::set_vote(&e, proposal_id, &voter, support);

    if support {
        proposal.votes_for += 1;
    } else {
        proposal.votes_against += 1;
    }

    store::set_proposal(&e, proposal_id, &proposal);

    e.events().publish(
        (symbol_short!("gov"), symbol_short!("vote")),
        (proposal_id, voter, support),
    );
}

/// Execute a proposal once voting has ended. If votes_for > votes_against, the admin is updated.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn execute_admin_proposal(e: Env, proposal_id: u64) {
    let mut proposal: AdminProposal = store::proposal(&e, proposal_id)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

    if proposal.status != ProposalStatus::Active {
        panic_with_error!(e, ContractError::ProposalNotActive);
    }

    let now = e.ledger().timestamp();
    if now <= proposal.end_time {
        panic_with_error!(e, ContractError::ProposalVotingPeriodNotEnded);
    }

    // Issue #864: reject execution if the admin has changed since this proposal
    // was created. Any admin drift — via update_admin, a timelocked SetAdmin,
    // or a prior proposal execution — invalidates the proposal.
    let current_admin = read_admin(&e);
    if current_admin != proposal.admin_at_creation {
        panic_with_error!(e, ContractError::StaleProposal);
    }

    if proposal.votes_for > proposal.votes_against {
        proposal.status = ProposalStatus::Executed;
        if crate::timelock::is_enabled(&e) {
            crate::timelock::schedule(
                e.clone(),
                TimelockAction::SetAdmin(proposal.proposed_admin.clone()),
            );
        } else {
            store::set_admin(&e, &proposal.proposed_admin);
            store::clear_pending_admin(&e);
        }

        e.events().publish(
            (symbol_short!("gov"), symbol_short!("executed")),
            (proposal_id, proposal.proposed_admin.clone()),
        );
    } else {
        proposal.status = ProposalStatus::Defeated;
        store::set_proposal(&e, proposal_id, &proposal);

        e.events().publish(
            (symbol_short!("gov"), symbol_short!("defeated")),
            proposal_id,
        );
    }

    store::set_proposal(&e, proposal_id, &proposal);
}

/// Cancel a governance proposal. Can be called by the proposer or current admin before execution.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn cancel_admin_proposal(e: Env, caller: Address, proposal_id: u64) {
    caller.require_auth();

    let mut proposal: AdminProposal = store::proposal(&e, proposal_id)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::ProposalNotFound));

    if proposal.status != ProposalStatus::Active {
        panic_with_error!(e, ContractError::ProposalNotActive);
    }

    let current_admin = read_admin(&e);
    if caller != proposal.proposer && caller != current_admin {
        panic_with_error!(e, ContractError::Unauthorized);
    }

    proposal.status = ProposalStatus::Cancelled;
    store::set_proposal(&e, proposal_id, &proposal);

    e.events().publish(
        (symbol_short!("gov"), symbol_short!("cancelled")),
        (proposal_id, caller),
    );
}

/// Retrieve a governance proposal by ID.
pub(crate) fn get_admin_proposal(e: &Env, proposal_id: u64) -> Option<AdminProposal> {
    store::proposal(e, proposal_id)
}

/// Query a voter's choice on a proposal.
pub(crate) fn get_admin_proposal_vote(e: &Env, proposal_id: u64, voter: Address) -> Option<bool> {
    store::vote(e, proposal_id, &voter)
}

/// Get total proposal count.
pub(crate) fn get_admin_proposal_count(e: &Env) -> u64 {
    store::proposal_count(e)
}
