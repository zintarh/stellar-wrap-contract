use soroban_sdk::{contracttype, Address, BytesN, Symbol};

/// The wrap record stored on-chain for each user
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub timestamp: u64,
    pub data_hash: BytesN<32>,
    pub archetype: Symbol,
    pub period: Symbol, // Period identifier (e.g., "2024-01" for monthly, "2024" for yearly)
}

/// Keys for persistent storage
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    AdminPubKey, 
    Wrap(Address, Symbol), // Address + Period identifier
    WrapCount(Address),    // Total wrap count for an address
}
