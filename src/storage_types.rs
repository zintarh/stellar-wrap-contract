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
    /// Stores the Address of the admin
    Admin,
    /// Stores the BytesN<32> public key for Ed25519 verification
    AdminPubKey,
    /// Stores individual WrapRecords (mapped by User and Period)
    /// Using u64 for period ensures consistent indexing
    Wrap(Address, u64),
    /// Stores the total number of wraps for a specific user (for balance_of)
    WrapCount(Address),
}
