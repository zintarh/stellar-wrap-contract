#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contracterror, contractimpl, Address, Bytes, BytesN, Env, Symbol,String,Vec};

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
    InvalidSignature = 5,
    SbtTransferNotAllowed = 6,
}

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    /// Initialize the contract with an admin address and public key. Only can be called once.
    /// 
    /// # Arguments
    /// * `admin` - The admin address
    /// * `admin_pubkey` - The admin's Ed25519 public key (32 bytes)
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) -> Result<(), Error> {
        let admin_key = DataKey::Admin;
        let pubkey_key = DataKey::AdminPubKey;

        // Ensure it's not already initialized
        if e.storage().instance().has(&admin_key) {
            return Err(Error::AlreadyInitialized);
        }

        e.storage().instance().set(&admin_key, &admin);
        e.storage().instance().set(&pubkey_key, &admin_pubkey);
        Ok(())
    }

    /// Verify that a signature was created by the admin
    /// 
    /// # Arguments
    /// * `payload` - The data that was signed
    /// * `signature` - The Ed25519 signature (64 bytes)
    /// 
    /// # Panics
    /// Panics if the signature is invalid or admin public key is not set
    pub fn verify_signature(e: Env, payload: Bytes, signature: BytesN<64>) -> Result<(), Error> {
        let pubkey_key = DataKey::AdminPubKey;

        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&pubkey_key)
            .ok_or(Error::NotInitialized)?;

        e.crypto().ed25519_verify(&admin_pubkey, &payload, &signature);

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

        // Increment wrap count for the user
        let count_key = DataKey::WrapCount(to.clone());
        let current_count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage().instance().set(&count_key, &(current_count + 1));

        // Emit event with topics ["mint", to_address, period] and data being the archetype
        use soroban_sdk::{symbol_short, IntoVal, Val};
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
    /// * `period` - Period identifier (e.g., "2024" for monthly, "2024" for yearly)
    pub fn get_wrap(e: Env, user: Address, period: Symbol) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user, period);
        e.storage().instance().get(&wrap_key)
    }

    /// Get the total wrap count for a user
    ///
    /// # Arguments
    /// * `user` - The user's address
    fn get_wrap_count(e: &Env, user: Address) -> u32 {
        let count_key = DataKey::WrapCount(user);
        e.storage().instance().get(&count_key).unwrap_or(0)
    }

    // ============================================================================
    // SEP-41 Token Interface Implementation (Read Functions)
    // These functions make the contract visible to standard Stellar wallets
    // ============================================================================

    /// Returns the balance (total wrap count) for a given address
    /// Implements SEP-41 token interface for wallet compatibility
    pub fn balance_of(e: Env, id: Address) -> i128 {
        Self::get_wrap_count(&e, id) as i128
    }

    /// Returns the number of decimals for this token
    /// Always returns 0 since wraps are indivisible items
    pub fn decimals(_e: Env) -> u32 {
        0
    }

    /// Returns the name of this token
    /// Implements SEP-41 token interface
    pub fn name(e: Env) -> String {
        String::from_str(&e, "Stellar Wrap Registry")
    }

    /// Returns the symbol of this token
    /// Implements SEP-41 token interface
    pub fn symbol(e: Env) -> String {
        String::from_str(&e, "WRAP")
    }

    /// Returns the allowance (always 0 for Soulbound Tokens)
    /// Implements SEP-41 token interface
    pub fn allowance(_e: Env, _from: Address, _spender: Address) -> i128 {
        0
    }

    // ============================================================================
    // SEP-41 Token Interface Implementation (Write Functions - Restricted)
    // These functions are required by the interface but must panic to enforce
    // the Soulbound Token (SBT) immutability property
    // ============================================================================

    /// Transfer wraps between addresses
    /// PANICS: Soulbound tokens cannot be transferred
    pub fn transfer(_e: Env, _from: Address, _to: Address, _amount: i128) {
        panic!("SBT: Transfer not allowed");
    }

    /// Transfer wraps on behalf of another address
    /// PANICS: Soulbound tokens cannot be transferred
    pub fn transfer_from(_e: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        panic!("SBT: Transfer not allowed");
    }

    /// Approve another address to spend wraps
    /// PANICS: Soulbound tokens cannot be transferred, so approval is meaningless
    pub fn approve(
        _e: Env,
        _from: Address,
        _spender: Address,
        _amount: i128,
        _expiration_ledger: u32,
    ) {
        panic!("SBT: Transfer not allowed");
    }

    /// Burn (destroy) wraps
    /// PANICS: Wrap history must remain immutable per issue requirements
    pub fn burn(_e: Env, _from: Address, _amount: i128) {
        panic!("SBT: Transfer not allowed");
    }
}

#[cfg(test)]
mod test;