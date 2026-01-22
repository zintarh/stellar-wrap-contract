use soroban_sdk::{contracttype, Address, BytesN, Symbol};

/// The wrap record stored on-chain for each user
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub minted_at: u64,        // Ledger timestamp
    pub archetype: Symbol,     // e.g., soroban_dev
    pub data_hash: BytesN<32>, // Hash of the JSON data
}

/// Keys for persistent storage
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,                      // Stores the admin Address
    Wrap(Address, u64),         // Maps (User, PeriodID) -> WrapRecord
    UserCount(Address),         // Maps User -> u32 (Total wraps owned)
}
