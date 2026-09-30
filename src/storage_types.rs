#[cfg(any(test, feature = "testutils"))]
extern crate std;

use soroban_sdk::{contracttype, Address, Bytes, BytesN, String, Symbol, Vec};

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WrapState {
    Draft = 1,
    Pending = 2,
    Active = 3,
    Archived = 4,
    Cancelled = 5,
    Expired = 6,
    Bridged = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapLifecycleFSM {
    pub state: WrapState,
    pub updated_at: u64,
}

impl WrapLifecycleFSM {
    pub fn new(initial_state: WrapState, now: u64) -> Self {
        Self {
            state: initial_state,
            updated_at: now,
        }
    }

    pub fn can_transition_to(&self, next: &WrapState) -> bool {
        matches!(
            (&self.state, next),
            (WrapState::Draft, WrapState::Pending)
                | (WrapState::Draft, WrapState::Cancelled)
                | (WrapState::Draft, WrapState::Expired)
                | (WrapState::Pending, WrapState::Active)
                | (WrapState::Bridged, WrapState::Active)
                | (WrapState::Pending, WrapState::Cancelled)
                | (WrapState::Pending, WrapState::Expired)
                | (WrapState::Active, WrapState::Pending)
                | (WrapState::Active, WrapState::Bridged)
                | (WrapState::Pending, WrapState::Bridged)
                | (WrapState::Active, WrapState::Archived)
                | (WrapState::Active, WrapState::Cancelled)
        )
    }

    pub fn transition_to(&mut self, next: WrapState, now: u64) -> bool {
        if self.can_transition_to(&next) {
            self.state = next;
            self.updated_at = now;
            true
        } else {
            false
        }
    }

    pub(crate) fn restore_from_bridge(&mut self, now: u64) -> bool {
        if self.state == WrapState::Bridged {
            self.state = WrapState::Active;
            self.updated_at = now;
            true
        } else {
            false
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub created_at: u64,
    pub data_hash: BytesN<32>,
    pub archetype: Symbol,
    pub period: u64,
    pub lifecycle: WrapLifecycleFSM,
    pub description: Option<String>,
    pub image_url: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchWrapItem {
    pub user: Address,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub signature: BytesN<64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractHealth {
    pub initialized: bool,
    pub has_admin: bool,
    pub has_signing_key: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeParams {
    pub base_fee: i128,
    pub per_kib_fee: i128,
    pub scale_step_kib: u64,
    pub max_fee: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimelockAction {
    SetAdmin(Address),
    SetAdminPubKey(BytesN<32>),
    Upgrade(BytesN<32>),
    SetWhitelistRoot(BytesN<32>),
    SetTimelockDelay(u64),
    SetBridgeRelayers(u32, BridgeRelayerSet),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockOperation {
    pub action: TimelockAction,
    pub eta: u64,
    pub scheduled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundBridgeRequest {
    pub nonce: u64,
    pub sender: Address,
    pub destination_chain: u32,
    pub recipient_address: Bytes,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundBridgeRecord {
    pub source_chain: u32,
    pub source_nonce: u64,
    pub recipient: Address,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BridgeRelayerSet {
    pub relayers: Vec<BytesN<32>>,
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferFeeConfig {
    pub amount: i128,
    pub recipient: Address,
    pub token: Address,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    AdminPubKey,
    PendingAdmin,
    Wrap(Address, u64),
    WrapCount(Address),
    LatestPeriod(Address),
    WrapPeriods(Address),
    TransferFee,
    TransferGuard,
    MigrationVersion,
    UserPeriods(Address),
    TotalWrapCount,
    TotalRevoked,
    AliasHash(Address),
    AliasOwner(BytesN<32>),
    Name,
    Symbol,
    Paused,
    ExpirationDuration,
    OptOut(Address),
    LastUpdated(Address),
    MintGuard(Address),
    MintedPeriod(Address, u32),
    StorageBytes,
    FeeParams,
    WhitelistRoot,
    TimelockDelay,
    TimelockOp(BytesN<32>),
    TimelockOps,
    BridgeRelayer,
    BridgeRelayerSet(u32),
    BridgeChainStatus(u32),
    OutboundBridgeNonce,
    OutboundBridgeRequest(u64),
    InboundBridgeProcessed(u32, u64),
    InboundBridgeRecord(u32, u64),
    AdminProposalCount,
    AdminProposal(u64),
    AdminProposalVote(u64, Address),
    ContractVersion,
    SchemaVersion,
    Stake(Address),
    StakeConfig,
    TotalStaked,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Active = 1,
    Executed = 2,
    Defeated = 3,
    Cancelled = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposal {
    pub id: u64,
    pub proposer: Address,
    pub proposed_admin: Address,
    pub votes_for: u64,
    pub votes_against: u64,
    pub start_time: u64,
    pub end_time: u64,
    pub status: ProposalStatus,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StakeConfig {
    pub min_stake: i128,
    pub cooldown_seconds: u64,
    pub priority_multiplier_bps: u32,
    pub max_priority_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakeRecord {
    pub amount: i128,
    pub staked_at: u64,
    pub unstaking_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantReport {
    pub wrap_count_match_user_periods: bool,
    pub wrap_count_match_wrap_periods: bool,
    pub latest_period_matches_max: bool,
    pub all_user_periods_live: bool,
    pub balance_matches_wrap_count: bool,
    pub wrap_count: u32,
    pub user_periods_len: u32,
    pub wrap_periods_len: u32,
    pub latest_period: Option<u64>,
    pub max_user_period: Option<u64>,
    pub live_wraps_found: u32,
    pub balance: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapSummary {
    pub total_wraps: u32,
    pub periods: Vec<u64>,
    pub archetypes: Vec<Symbol>,
    pub first_period: u64,
    pub latest_period: u64,
}
