#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env,
    String, Symbol,
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
    InvalidSignature = 5,
}

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    /// Initialize the contract with an admin address and public key.
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) -> Result<(), Error> {
        if e.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::AdminPubKey, &admin_pubkey);
        Ok(())
    }

    /// The core write-action: Users claim their wrap using an Admin signature.
    pub fn mint_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
    ) -> Result<(), Error> {
        // 1. Reconstruct Payload (Order: Contract -> User -> Period -> Archetype -> Data Hash)
        // We use .clone() here because .to_xdr() consumes the value.
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e)); // Added .clone()
        payload.append(&period.to_xdr(&e)); // u64 implements Copy, so no clone needed
        payload.append(&archetype.clone().to_xdr(&e)); // Added .clone()
        payload.append(&data_hash.clone().to_xdr(&e)); // Added .clone()

        // 2. Verify Signature
        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .ok_or(Error::NotInitialized)?;

        // Verify the signature against the payload
        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        // 3. Check Duplicates
        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().instance().has(&wrap_key) {
            return Err(Error::WrapAlreadyExists);
        }

        // 4. Store Record
        let record = WrapRecord {
            timestamp: e.ledger().timestamp(),
            data_hash, // Ownership moves here (last use)
            archetype: archetype.clone(),
            period,
        };
        e.storage().instance().set(&wrap_key, &record);

        // 5. Update Balance (UserCount)
        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().instance().get(&count_key).unwrap_or(0);
        e.storage().instance().set(&count_key, &(current_count + 1));

        // 6. Emit Event: ["mint", user, period]
        e.events()
            .publish((symbol_short!("mint"), user, period), archetype);

        Ok(())
    }

    // --- Read Functions (SEP-41 Compatibility) ---

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

    pub fn name(e: Env) -> String {
        String::from_str(&e, "Stellar Wrap Registry")
    }

    pub fn symbol(e: Env) -> String {
        String::from_str(&e, "WRAP")
    }

    pub fn decimals(_e: Env) -> u32 {
        0
    }

    pub fn get_admin(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Admin)
    }
}

mod test;