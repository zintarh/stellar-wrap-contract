#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contractmeta, contracttype, panic_with_error,
    symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol,
};

contractmeta!(
    key = "name",
    val = "Stellar Wrap Registry"
);
contractmeta!(
    key = "version",
    val = "0.1.0"
);
contractmeta!(
    key = "repo",
    val = "https://github.com/calebux/stellar-wrap-contract"
);
contractmeta!(
    key = "description",
    val = "On-chain Soulbound Token registry for Stellar Wrap. Stores non-transferable wrap records linked to user addresses."
);

#[contracttype]
pub struct ContractInfo {
    pub name: String,
    pub version: String,
    pub repo: String,
    pub description: String,
}

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

    /// Update the admin address. Only callable by the current admin.
    pub fn update_admin(e: Env, new_admin: Address) {
        let current_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        current_admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Users claim their wrap using an Admin signature.
    pub fn mint_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
    ) {
        // 1. Security: Ensure the user actually signed this transaction
        user.require_auth();

        // 2. Verify initialization
        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        // 3. Reconstruct Payload
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&archetype.clone().to_xdr(&e));
        payload.append(&data_hash.clone().to_xdr(&e));

        // 4. Verify Admin Signature
        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        // 5. Check Duplicates & Store Record (Switch to Persistent)
        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().persistent().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapAlreadyExists);
        }

        let record = WrapRecord {
            timestamp: e.ledger().timestamp(),
            data_hash,
            archetype: archetype.clone(),
            period,
        };

        // Store in persistent and extend TTL to ~1 year
        let ttl_one_year = 17280 * 365;
        e.storage().persistent().set(&wrap_key, &record);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, ttl_one_year, ttl_one_year);

        // 6. Update Balance (Switch to Persistent)
        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
        e.storage()
            .persistent()
            .set(&count_key, &(current_count + 1));
        e.storage()
            .persistent()
            .extend_ttl(&count_key, ttl_one_year, ttl_one_year);

        // 6b. Track latest period for get_latest_wrap
        let latest_key = DataKey::LatestPeriod(user.clone());
        let current_latest: u64 = e.storage().persistent().get(&latest_key).unwrap_or(0);
        if period > current_latest {
            e.storage().persistent().set(&latest_key, &period);
            e.storage()
                .persistent()
                .extend_ttl(&latest_key, ttl_one_year, ttl_one_year);
        }

        // 7. Emit Event
        e.events()
            .publish((symbol_short!("mint"), user, period), archetype);
    }

    // --- Read Functions ---

    pub fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
        // Changed .instance() to .persistent() to match mint_wrap
        e.storage().persistent().get(&DataKey::Wrap(user, period))
    }

    pub fn balance_of(e: Env, id: Address) -> i128 {
        let count_key = DataKey::WrapCount(id);
        // Changed .instance() to .persistent() to match mint_wrap
        e.storage()
            .persistent()
            .get::<_, u32>(&count_key)
            .unwrap_or(0) as i128
    }

    pub fn verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool {
        let wrap: Option<WrapRecord> = e.storage().persistent().get(&DataKey::Wrap(user, period));
        match wrap {
            Some(record) => {
                let computed_hash = e.crypto().sha256(&data);
                record.data_hash == BytesN::from_array(&e, &computed_hash.to_array())
            }
            None => false,
        }
    }

    pub fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
        let latest_key = DataKey::LatestPeriod(user.clone());
        let period: u64 = e.storage().persistent().get(&latest_key)?;
        e.storage().persistent().get(&DataKey::Wrap(user, period))
    }

    /// Extend TTL for a user's wrap record and instance storage.
    /// Public — anyone can call this to keep records alive.
    pub fn extend_ttl(e: Env, user: Address, period: u64) {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let ttl = 17280 * 365; // ~1 year in ledgers

        if e.storage().persistent().has(&wrap_key) {
            e.storage().persistent().extend_ttl(&wrap_key, ttl, ttl);
        }

        let count_key = DataKey::WrapCount(user.clone());
        if e.storage().persistent().has(&count_key) {
            e.storage().persistent().extend_ttl(&count_key, ttl, ttl);
        }

        let latest_key = DataKey::LatestPeriod(user);
        if e.storage().persistent().has(&latest_key) {
            e.storage().persistent().extend_ttl(&latest_key, ttl, ttl);
        }

        e.storage().instance().extend_ttl(ttl, ttl);
    }

    pub fn get_admin(e: Env) -> Option<Address> {
        // This stays .instance() because initialize() uses instance()
        e.storage().instance().get(&DataKey::Admin)
    }

    pub fn contract_info(e: Env) -> ContractInfo {
        ContractInfo {
            name: String::from_str(&e, "Stellar Wrap Registry"),
            version: String::from_str(&e, "0.1.0"),
            repo: String::from_str(&e, "https://github.com/calebux/stellar-wrap-contract"),
            description: String::from_str(
                &e,
                "On-chain Soulbound Token registry for Stellar Wrap.",
            ),
        }
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

#[cfg(test)]
mod security_test;
#[cfg(test)]
mod test;
