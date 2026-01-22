#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracterror, Address, BytesN, Env, Symbol, Vec,
};

mod storage_types;
use storage_types::{DataKey, WrapRecord};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    WrapAlreadyExists = 4,
}

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    /// Initialize the contract with an admin. Only can be called once.
    pub fn initialize(e: Env, admin: Address) -> Result<(), Error> {
        let key = DataKey::Admin;
        
        // Ensure it's not already initialized
        if e.storage().instance().has(&key) {
            return Err(Error::AlreadyInitialized);
        }
        
        e.storage().instance().set(&key, &admin);
        Ok(())
    }

    /// Mint a wrap record for `to` for a specific period. Only callable by admin.
    /// 
    /// # Arguments
    /// * `to` - The address to mint the wrap for
    /// * `data_hash` - SHA256 hash of the full off-chain JSON data
    /// * `archetype` - The persona archetype assigned to the user (e.g., soroban_dev)
    /// * `period_id` - Period identifier (u64)
    pub fn mint_wrap(
        e: Env,
        to: Address,
        data_hash: BytesN<32>,
        archetype: Symbol,
        period_id: u64,
    ) -> Result<(), Error> {
        // Get and verify admin
        let admin_key = DataKey::Admin;
        let admin: Address = e
            .storage()
            .instance()
            .get(&admin_key)
            .ok_or(Error::NotInitialized)?;
        
        // Verify caller is admin
        admin.require_auth();
        
        // Check if wrap already exists for this user and period
        let wrap_key = DataKey::Wrap(to.clone(), period_id);
        if e.storage().instance().has(&wrap_key) {
            return Err(Error::WrapAlreadyExists);
        }
        
        // Get current ledger timestamp
        let minted_at = e.ledger().timestamp();
        
        // Create the wrap record
        let record = WrapRecord {
            minted_at,
            data_hash,
            archetype: archetype.clone(),
        };
        
        // Store the record
        e.storage().instance().set(&wrap_key, &record);
        
        // Increment user count
        let user_count_key = DataKey::UserCount(to.clone());
        let current_count: u32 = e.storage().instance().get(&user_count_key).unwrap_or(0);
        e.storage().instance().set(&user_count_key, &(current_count + 1));
        
        // Emit event with topics ["mint", to_address, period_id] and data being the archetype
        use soroban_sdk::{symbol_short, IntoVal, Val};
        let mut topics: Vec<Val> = Vec::new(&e);
        topics.push_back(symbol_short!("mint").into_val(&e));
        topics.push_back(to.clone().into_val(&e));
        topics.push_back(period_id.into_val(&e));
        e.events().publish((topics,), archetype);
        
        Ok(())
    }

    /// Retrieve the wrap record for a user for a specific period, if any
    /// 
    /// # Arguments
    /// * `user` - The user's address
    /// * `period_id` - Period identifier (u64)
    pub fn get_wrap(e: Env, user: Address, period_id: u64) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user, period_id);
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

#[cfg(test)]
mod test;
