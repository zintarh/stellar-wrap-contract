#![no_std]

use soroban_sdk::{
    contract,
    contractimpl,
    contracterror,
    panic_with_error,
    Address,
    BytesN,
    Env,
    Symbol,
    Vec,
    Val,
    IntoVal,
};

mod storage_types;
use storage_types::{DataKey, WrapRecord};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    WrapAlreadyExists = 4,
    InvalidSignature = 5,
}

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    pub fn initialize(e: Env, admin: Address) {
        let key = DataKey::Admin;

        if e.storage().instance().has(&key) {
            panic_with_error!(e, ContractError::AlreadyInitialized);
        }

        e.storage().instance().set(&key, &admin);
    }

    pub fn mint_wrap(
        e: Env,
        to: Address,
        data_hash: BytesN<32>,
        archetype: Symbol,
        period: Symbol,
    ) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        admin.require_auth();

        if !verify_signature(&data_hash) {
            panic_with_error!(e, ContractError::InvalidSignature);
        }

        let wrap_key = DataKey::Wrap(to.clone(), period.clone());
        if e.storage().instance().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapAlreadyExists);
        }

        let timestamp = e.ledger().timestamp();

        let record = WrapRecord {
            minted_at,
            data_hash,
            archetype: archetype.clone(),
        };

        e.storage().instance().set(&wrap_key, &record);

        // Emit event with period as u64
        use soroban_sdk::symbol_short;

        let topics: Vec<Val> = Vec::from_array(
            &e,
            [
                symbol_short!("mint").into_val(&e),
                to.clone().into_val(&e),
            ],
        );

        // Convert Symbol to a simple u64 hash for the event data
        let period_u64 = symbol_to_u64(&period);
        
        e.events().publish(topics, period_u64);
    }

    pub fn get_wrap(e: Env, user: Address, period: Symbol) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user, period);
        e.storage().instance().get(&wrap_key)
    }

    /// Get the total count of wraps owned by a user
    /// 
    /// # Arguments
    /// * `user` - The user's address
    pub fn get_user_count(e: Env, user: Address) -> u32 {
        let user_count_key = DataKey::UserCount(user);
        e.storage().instance().get(&user_count_key).unwrap_or(0)
    }
}

fn verify_signature(_data_hash: &BytesN<32>) -> bool {
    true
}

fn symbol_to_u64(symbol: &Symbol) -> u64 {
    let val: Val = symbol.to_val();
    val.get_payload()
}

#[cfg(test)]
mod test;