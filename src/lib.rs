#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contracterror, contractimpl, Address, BytesN, Env, Symbol};

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
    /// * `archetype` - The persona archetype assigned to the user
    /// * `period` - Period identifier (e.g., "2024-01" for monthly, "2024" for yearly)
    pub fn mint_wrap(
        e: Env,
        to: Address,
        data_hash: BytesN<32>,
        archetype: Symbol,
        period: Symbol,
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
        let wrap_key = DataKey::Wrap(to.clone(), period.clone());
        if e.storage().instance().has(&wrap_key) {
            return Err(Error::WrapAlreadyExists);
        }

        // Get current ledger timestamp
        let timestamp = e.ledger().timestamp();

        // Create the wrap record
        let record = WrapRecord {
            timestamp,
            data_hash,
            archetype: archetype.clone(),
            period: period.clone(),
        };

        // Store the record
        e.storage().instance().set(&wrap_key, &record);

        // Emit event with topics ["mint", to_address, period] and data being the archetype
        use soroban_sdk::{symbol_short, IntoVal, Val, Vec};
        let topics: Vec<Val> = Vec::from_array(
            &e,
            [
                symbol_short!("mint").into_val(&e),
                to.clone().into_val(&e),
                period.into_val(&e),
            ],
        );
        e.events().publish((topics,), archetype);

        Ok(())
    }

    /// Retrieve the wrap record for a user for a specific period, if any
    ///
    /// # Arguments
    /// * `user` - The user's address
    /// * `period` - Period identifier (e.g., "2024-01" for monthly, "2024" for yearly)
    pub fn get_wrap(e: Env, user: Address, period: Symbol) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user, period);
        e.storage().instance().get(&wrap_key)
    }
}

#[cfg(test)]
mod test;
