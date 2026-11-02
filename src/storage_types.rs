#[cfg(any(test, feature = "testutils"))]
extern crate std;

use soroban_sdk::{contracttype, Address, Bytes, BytesN, String, Symbol, Vec};

/// Minimal reachable lifecycle for a wrap record.
///
/// Every variant is reachable from a non-admin product flow:
/// - `Active`: created by `mint_wrap`, `mint_wrap_batch`, `bridge_wrap_in`,
///   and restored by `bridge_wrap_out` completion.
/// - `Bridged`: set by `bridge_wrap_out` while the record is in flight.
/// - `Expired`: set by `expire_wrap` once the expiration duration elapses.
///
/// Revoke and burn are deletions (with events), not states.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WrapState {
    Active = 1,
    Bridged = 2,
    Expired = 3,
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

    /// Documented transition table (see `docs/lifecycle.md`):
    ///
    /// | from    | to      | trigger          |
    /// |---------|---------|------------------|
    /// | Active  | Bridged | bridge_wrap_out  |
    /// | Bridged | Active  | bridge_wrap_in   |
    /// | Active  | Expired | expire_wrap      |
    /// | Bridged | Expired | expire_wrap      |
    pub fn can_transition_to(&self, next: &WrapState) -> bool {
        matches!(
            (&self.state, next),
            (WrapState::Active, WrapState::Bridged)
                | (WrapState::Bridged, WrapState::Active)
                | (WrapState::Active, WrapState::Expired)
                | (WrapState::Bridged, WrapState::Expired)
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
    /// Timestamp associated with the wrap record.
    pub created_at: u64,
    /// 32-byte hash associated with the wrapped data.
    pub data_hash: BytesN<32>,
    /// Symbol identifying the wrap's archetype.
    pub archetype: Symbol,
    /// Period identifier used with the user to address this record in persistent storage.
    pub period: u64,
    /// Current lifecycle state and its last update timestamp.
    pub lifecycle: WrapLifecycleFSM,
    /// Optional human-readable description attached by the admin.
    pub description: Option<String>,
    /// Optional image URL attached by the admin.
    pub image_url: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapView {
    pub record: WrapRecord,
    pub is_expired: bool,
    pub live_until_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchWrapItem {
    pub user: Address,
    pub period: u64,
    pub archetype: Symbol,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub valid_until: u64,
    pub signature: BytesN<64>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractHealth {
    /// Whether `initialize()` has been called (admin address is set).
    pub initialized: bool,
    /// Whether an admin address is currently configured.
    pub has_admin: bool,
    /// Whether an admin signing (public) key is currently configured.
    pub has_signing_key: bool,
}

/// New struct: FeeParams for algorithmic fee model
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeParams {
    /// base fee in token units
    pub base_fee: i128,
    /// fee increment per scaling step (applied per `scale_step_kib`)
    pub per_kib_fee: i128,
    /// scaling step in KiB (e.g., 1024 means per KiB)
    pub scale_step_kib: u64,
    /// maximum fee cap
    pub max_fee: i128,
}

/// A privileged action that can only take effect after the timelock delay.
///
/// Every variant maps to exactly one state mutation applied by
/// `timelock::execute`. Keeping the set closed means no scheduled operation can
/// smuggle in a call the contract does not already expose.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimelockAction {
    /// Replace the admin address.
    SetAdmin(Address),
    /// Rotate the Ed25519 signing key used to validate mint payloads.
    SetAdminPubKey(BytesN<32>),
    /// Upgrade the contract WASM to the given hash.
    Upgrade(BytesN<32>),
    /// Publish a new off-chain whitelist merkle root.
    SetWhitelistRoot(BytesN<32>),
    /// Change the timelock delay itself (seconds).
    SetTimelockDelay(u64),
    /// Configure the bridge relayer set and threshold for a given chain.
    SetBridgeRelayers(u32, BridgeRelayerSet),
}

/// A scheduled timelock operation awaiting execution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockOperation {
    /// The action to apply once `eta` is reached.
    pub action: TimelockAction,
    /// Ledger timestamp at which the action becomes executable.
    pub eta: u64,
    /// Ledger timestamp at which the action was scheduled.
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
    /// Amount of `token` charged to the sender for each successful transfer.
    pub amount: i128,
    /// Address that receives transfer fees.
    pub recipient: Address,
    /// Soroban token contract used to collect fees.
    pub token: Address,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Stores the address of the admin.
    Admin,
    /// Stores the Ed25519 public key used to validate backend signatures.
    AdminPubKey,
    /// Stores a proposed new admin address during two-step transfer.
    PendingAdmin,
    /// Stores individual wrap records keyed by user and period.
    Wrap(Address, u64),
    /// Stores the total number of wraps for a specific user.
    WrapCount(Address),
    /// Stores the latest period minted for a specific user.
    LatestPeriod(Address),
    /// Stores the periods currently owned by a user so transfers can update
    /// `LatestPeriod` without scanning contract storage.
    WrapPeriods(Address),
    /// Stores the admin-controlled transfer fee configuration.
    TransferFee,
    /// Temporary reentrancy guard for transfer calls.
    TransferGuard,
    /// Stores the highest storage migration version already applied.
    MigrationVersion,
    /// Stores the periods a user has minted, in insertion order. `get_wraps`
    /// returns records in this order, not sorted by period.
    UserPeriods(Address),
    /// Total number of wraps ever minted across all users (lifetime counter).
    /// This is intentionally NOT decremented on `revoke_wrap` or `burn_wrap`.
    TotalWrapCount,
    /// Stores the total number of wrap records revoked on-chain.
    TotalRevoked,
    /// Stores a user-controlled 32-byte alias hash for privacy-preserving profile display.
    AliasHash(Address),
    /// Stores the 32-byte hash that owns a given alias.
    AliasOwner(BytesN<32>),
    /// Stores the token display name, if overridden by an admin.
    /// Falls back to a hardcoded default when unset — see `queries::name`.
    Name,
    /// Stores the token symbol, if overridden by an admin.
    /// Falls back to a hardcoded default when unset — 
