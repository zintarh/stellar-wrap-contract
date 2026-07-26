use soroban_sdk::{contracttype, Address, BytesN, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub timestamp: u64,
    pub data_hash: BytesN<32>,
    pub archetype: Symbol,
    pub period: u64, // Standardized to u64 for better indexing/sorting
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Stores the address of the admin.
    Admin,
    /// Stores the Ed25519 public key used to validate backend signatures.
    AdminPubKey,
    /// Stores individual wrap records keyed by user and period.
    Wrap(Address, u64),
    /// Stores the total number of wraps for a specific user.
    WrapCount(Address),
    /// Stores the latest period minted for a specific user.
    LatestPeriod(Address),
}
