use soroban_sdk::{contracttype, Address, BytesN, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrapRecord {
    pub minted_at: u64,
    pub archetype: Symbol,
    pub period: Symbol,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Wrap(Address, Symbol),
}
