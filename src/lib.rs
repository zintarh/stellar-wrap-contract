#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, xdr::ToXdr, Address,
    Bytes, BytesN, Env, String, Symbol,
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
    /// Initialize with admin and the public key used to verify off-chain signatures.
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(e, ContractError::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::AdminPubKey, &admin_pubkey);
    }

    /// Users claim their wrap using an Admin signature (from HEAD).
    pub fn mint_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
    ) {
        // 1. Verify initialization
        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        // 2. Reconstruct Payload (Contract -> User -> Period -> Archetype -> Data Hash)
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&archetype.clone().to_xdr(&e));
        payload.append(&data_hash.clone().to_xdr(&e));

        // 3. Verify Signature
        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        // 4. Check Duplicates
        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().instance().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapAlreadyExists);
        }

        // 5. Store Record
        let record = WrapRecord {
            timestamp: e.ledger().timestamp(),
            data_hash,
            archetype: archetype.clone(),
            period,
        };
        e.storage().instance().set(&wrap_key, &record);

        // 6. Update Balance (User Registry Count)
        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage().instance().set(&count_key, &(current_count + 1));

        // 7. Emit Event (Using the structured topics from main)
        e.events()
            .publish((symbol_short!("mint"), user, period), archetype);
    }

    // --- Read Functions (SEP-41 Compatibility & Helpers) ---

    pub fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
        e.storage().instance().get(&DataKey::Wrap(user, period))
    }

    pub fn balance_of(e: Env, id: Address) -> i128 {
        let count_key = DataKey::WrapCount(id);
        e.storage()
            .instance()
            .get::<_, u32>(&count_key)
            .unwrap_or(0) as i128
    }

    pub fn get_admin(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Admin)
    }

    pub fn name(e: Env) -> String {
        String::from_str(&e, "Stellar Wrap Registry")
    }

    pub fn symbol(e: Env) -> String {
        String::from_str(&e, "WRAP")
    }

    pub fn decimals(_e: Env) -> u32 {
        0
    }
}

mod test;
