use soroban_sdk::{contracttype, Address, BytesN, String, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractInfo {
    pub name: String,
    pub version: String,
    pub repo: String,
    pub description: String,
}

/// Schema v1 wrap record (no `image_uri`). Retained for lazy migration reads.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecordV1 {
    pub timestamp: u64,
    pub data_hash: BytesN<32>,
    pub archetype: Symbol,
    pub period: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub timestamp: u64,
    pub data_hash: BytesN<32>,
    pub archetype: Symbol,
    pub period: u64, // Standardized to u64 for better indexing/sorting
    /// Optional off-chain image URI (schema v2+). Empty for legacy records.
    pub image_uri: String,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Stores the Address of the admin
    Admin,
    /// Stores the BytesN<32> public key for Ed25519 verification
    AdminPubKey,
    /// Current storage schema version (instance storage)
    SchemaVersion,
    /// Stores individual WrapRecords (mapped by User and Period)
    /// Using u64 for period ensures consistent indexing
    Wrap(Address, u64),
    /// Stores the total number of wraps for a specific user (for balance_of)
    WrapCount(Address),
    /// Tracks the latest (highest) period minted for a user
    LatestPeriod(Address),
    /// Temporary, invocation-scoped reentrancy guard for mint flow
    MintGuard(Address),
    /// Merkle root for batch claims per period
    MerkleRoot(u64),
    /// Tracks whether a user has claimed via merkle for a period
    MerkleClaimed(Address, u64),
    /// User privacy opt-out flag (persistent)
    UserOptOut(Address),
}

/// Current schema version written by `initialize()` and advanced by `migrate()`.
pub const SCHEMA_VERSION: u32 = 1;
/// Target schema version after v1 → v2 migration (`image_uri` field).
pub const SCHEMA_VERSION_V2: u32 = 2;
