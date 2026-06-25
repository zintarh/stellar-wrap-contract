#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, xdr::ToXdr, Address,
    Bytes, BytesN, Env, String, Symbol,
};

mod storage_types;
use storage_types::{DataKey, WrapRecord};

soroban_sdk::contractmeta!(
    key = "Description",
    val = "Soulbound token registry for Stellar Wrap"
);
soroban_sdk::contractmeta!(key = "Version", val = "0.1.0");
soroban_sdk::contractmeta!(key = "Name", val = "Stellar Wrap Registry");
soroban_sdk::contractmeta!(key = "Author", val = "Stellar Wrap Team");

/// Errors returned by the StellarWrap contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// `initialize()` was called on a contract that is already initialized. (code 1)
    AlreadyInitialized = 1,
    /// A function was called before `initialize()` has been run. (code 2)
    NotInitialized = 2,
    /// The caller is not the admin. (code 3)
    Unauthorized = 3,
    /// A wrap record for this `(user, period)` pair already exists. (code 4)
    WrapAlreadyExists = 4,
    /// The wrap record was not found. (code 5)
    WrapNotFound = 5,
    /// The provided Ed25519 signature did not verify against the admin public key. (code 6)
    InvalidSignature = 6,
    /// `data_hash` is all-zero bytes, which indicates missing or invalid data. (code 7)
    InvalidDataHash = 7,
}

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    /// Initialize the contract with an admin address and the Ed25519 public key used to
    /// verify off-chain wrap signatures.
    ///
    /// # Parameters
    /// - `admin`: The `Address` that will have privileged control (upgrade, update_admin).
    /// - `admin_pubkey`: The 32-byte Ed25519 public key whose private key signs wrap payloads.
    ///
    /// # Panics
    /// - [`ContractError::AlreadyInitialized`] if called more than once.
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
        if e.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(e, ContractError::AlreadyInitialized);
        }
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage()
            .instance()
            .set(&DataKey::AdminPubKey, &admin_pubkey);
    }

    /// Replace the current admin with a new address.
    ///
    /// # Parameters
    /// - `new_admin`: The `Address` that will become the new admin.
    ///
    /// # Authorization
    /// Requires authorization from the **current** admin.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    pub fn update_admin(e: Env, new_admin: Address) {
        let current_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        current_admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events().publish(
            (symbol_short!("admin"), symbol_short!("updated")),
            (current_admin, new_admin),
        );
    }

    /// Mint a soulbound wrap record for a user.
    ///
    /// The backend generates a payload of `(contract_id ‖ user ‖ period ‖ archetype ‖ data_hash)`,
    /// signs it with the admin Ed25519 private key, and delivers the signature to the user.
    /// The user then calls this function to claim their on-chain wrap record.
    ///
    /// **Invariant:** The admin must issue at most one signature per `(user, period)` pair.
    /// Only one archetype can ever be stored for a given period; a second valid signature for
    /// the same user+period with a different archetype is permanently unusable.
    /// See [Issue #31](https://github.com/zintarh/stellar-wrap-contract/issues/31).
    ///
    /// # Parameters
    /// - `user`: The `Address` receiving the wrap. Must authorize this call.
    /// - `period`: A `u64` identifier for the wrap period (e.g. `202412` for December 2024).
    /// - `archetype`: A short `Symbol` describing the user's persona (e.g. `"builder"`).
    /// - `data_hash`: SHA-256 hash of the off-chain JSON data. Must not be all-zero bytes.
    /// - `signature`: 64-byte Ed25519 signature from the admin over the canonical payload.
    ///
    /// # Returns
    /// Nothing on success. Emits a `(mint, user, period) → archetype` event.
    ///
    /// # Authorization
    /// Requires authorization from `user`.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - [`ContractError::InvalidDataHash`] if `data_hash` is all-zero bytes.
    /// - [`ContractError::InvalidSignature`] if the Ed25519 signature is invalid.
    /// - [`ContractError::WrapAlreadyExists`] if a wrap for `(user, period)` already exists.
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

        // 1b. Reentrancy guard in temporary storage.
        // If execution panics, the temporary TTL naturally clears stale entries.
        let guard_key = DataKey::MintGuard(user.clone());
        if e.storage().temporary().has(&guard_key) {
            panic_with_error!(e, ContractError::Unauthorized);
        }
        e.storage().temporary().set(&guard_key, &true);

        // 2. Verify initialization
        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        // 3. Reject zero data_hash — all-zero bytes indicate missing or invalid data
        if data_hash == BytesN::from_array(&e, &[0u8; 32]) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        // 4. Reconstruct payload: contract_id ‖ user ‖ period ‖ archetype ‖ data_hash
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&archetype.clone().to_xdr(&e));
        payload.append(&data_hash.clone().to_xdr(&e));

        // 5. Verify Admin Signature
        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        // 6. Check Duplicates & Store Record
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

        // 7. Update Balance
        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
        e.storage()
            .persistent()
            .set(&count_key, &(current_count + 1));
        e.storage()
            .persistent()
            .extend_ttl(&count_key, ttl_one_year, ttl_one_year);

        // 7b. Track latest period for get_latest_wrap
        let latest_key = DataKey::LatestPeriod(user.clone());
        let current_latest: u64 = e.storage().persistent().get(&latest_key).unwrap_or(0);
        if period > current_latest {
            e.storage().persistent().set(&latest_key, &period);
            e.storage()
                .persistent()
                .extend_ttl(&latest_key, ttl_one_year, ttl_one_year);
        }

        // Clear guard on successful completion.
        e.storage().temporary().remove(&guard_key);

        // 8. Emit Event
        e.events()
            .publish((symbol_short!("mint"), user, period), archetype);
    }

    /// Update an existing wrap record's data_hash and archetype (admin-only).
    ///
    /// The original `timestamp` is preserved. A new `update` event is emitted.
    ///
    /// # Parameters
    /// - `user`: The address whose wrap record is being updated.
    /// - `period`: The period identifier of the record to update.
    /// - `new_data_hash`: Replacement SHA-256 hash. Must not be all-zero bytes.
    /// - `new_archetype`: Replacement archetype symbol.
    /// - `signature`: 64-byte Ed25519 signature from the admin over the new payload.
    ///
    /// # Authorization
    /// Requires authorization from the **admin**.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - [`ContractError::Unauthorized`] if the caller is not the admin.
    /// - [`ContractError::InvalidDataHash`] if `new_data_hash` is all-zero bytes.
    /// - [`ContractError::InvalidSignature`] if the Ed25519 signature is invalid.
    /// - [`ContractError::WrapNotFound`] if no wrap exists for `(user, period)`.
    pub fn update_wrap(
        e: Env,
        user: Address,
        period: u64,
        new_data_hash: BytesN<32>,
        new_archetype: Symbol,
        signature: BytesN<64>,
    ) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if new_data_hash == BytesN::from_array(&e, &[0u8; 32]) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        // Payload: contract_id ‖ user ‖ period ‖ new_archetype ‖ new_data_hash
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&new_archetype.clone().to_xdr(&e));
        payload.append(&new_data_hash.clone().to_xdr(&e));
        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        let wrap_key = DataKey::Wrap(user.clone(), period);
        let existing: WrapRecord = e
            .storage()
            .persistent()
            .get(&wrap_key)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

        let updated = WrapRecord {
            timestamp: existing.timestamp, // preserve original timestamp
            data_hash: new_data_hash,
            archetype: new_archetype.clone(),
            period,
        };

        let ttl_one_year = 17280 * 365;
        e.storage().persistent().set(&wrap_key, &updated);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, ttl_one_year, ttl_one_year);

        e.events()
            .publish((symbol_short!("update"), user, period), new_archetype);
    }

    /// Admin-only revocation for incorrect or fraudulent records.
    pub fn revoke_wrap(e: Env, user: Address, period: u64) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        let wrap_key = DataKey::Wrap(user.clone(), period);
        if !e.storage().persistent().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapNotFound);
        }

        e.storage().persistent().remove(&wrap_key);

        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
        if current_count > 0 {
            e.storage()
                .persistent()
                .set(&count_key, &(current_count - 1));
        }

        e.events()
            .publish((symbol_short!("revoke"), user, period), true);
    }

    // --- Read Functions ---

    /// Retrieve the wrap record for a specific `(user, period)` pair.
    ///
    /// # Parameters
    /// - `user`: The address whose wrap record is requested.
    /// - `period`: The period identifier to look up.
    ///
    /// # Returns
    /// `Some(WrapRecord)` if a record exists, `None` otherwise.
    pub fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
        e.storage().persistent().get(&DataKey::Wrap(user, period))
    }

    /// Return the total number of wrap records minted for a user (their SBT balance).
    ///
    /// # Parameters
    /// - `id`: The address to query.
    ///
    /// # Returns
    /// The number of wraps as `i128`. Returns `0` if the user has no wraps or the contract
    /// has not been initialized.
    pub fn balance_of(e: Env, id: Address) -> i128 {
        let count_key = DataKey::WrapCount(id);
        e.storage()
            .persistent()
            .get::<_, u32>(&count_key)
            .unwrap_or(0) as i128
    }

    /// Verify that the SHA-256 hash of `data` matches the `data_hash` stored in a wrap record.
    ///
    /// Useful for off-chain integrity checks: hash the original JSON off-chain, then call this
    /// to confirm the on-chain record matches without re-uploading the full data.
    ///
    /// # Parameters
    /// - `user`: The address whose wrap record is checked.
    /// - `period`: The period identifier to look up.
    /// - `data`: The raw bytes whose SHA-256 will be compared against the stored hash.
    ///
    /// # Returns
    /// `true` if the hash matches, `false` if it does not or if no record exists.
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

    /// Return the most recent wrap record minted for a user (highest period value).
    ///
    /// # Parameters
    /// - `user`: The address to query.
    ///
    /// # Returns
    /// `Some(WrapRecord)` for the latest period, or `None` if the user has no wraps.
    pub fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
        let latest_key = DataKey::LatestPeriod(user.clone());
        let period: u64 = e.storage().persistent().get(&latest_key)?;
        e.storage().persistent().get(&DataKey::Wrap(user, period))
    }

    /// Extend the TTL (time-to-live) for all persistent storage entries belonging to a user.
    ///
    /// Soroban persistent storage entries expire after their TTL lapses. This function lets
    /// anyone renew a user's wrap records so they remain accessible indefinitely.
    ///
    /// # Parameters
    /// - `user`: The address whose storage entries will be extended.
    /// - `period`: The specific wrap period whose record TTL will be extended.
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

    /// Return the current admin address, or `None` if the contract is not yet initialized.
    pub fn get_admin(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Admin)
    }

    /// Return the human-readable name of this token registry.
    ///
    /// # Returns
    /// `"Stellar Wrap Registry"`
    pub fn name(e: Env) -> String {
        String::from_str(&e, "Stellar Wrap Registry")
    }

    /// Return the ticker symbol for this token registry.
    ///
    /// # Returns
    /// `"WRAP"`
    pub fn symbol(e: Env) -> String {
        String::from_str(&e, "WRAP")
    }

    /// Return the number of decimals. Soulbound tokens are non-divisible, so this is always `0`.
    pub fn decimals(_e: Env) -> u32 {
        0
    }

    /// Upgrade the contract WASM to a new version.
    ///
    /// The Soroban runtime validates the WASM hash against the uploaded blob in the ledger.
    /// After a successful upgrade, subsequent invocations run the new WASM code while all
    /// persistent storage (wrap records, admin key, etc.) is preserved.
    ///
    /// # Parameters
    /// - `new_wasm_hash`: The 32-byte hash of the new WASM blob as uploaded to the network.
    ///
    /// # Authorization
    /// Requires authorization from the **current** admin.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    pub fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        admin.require_auth();
        e.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[cfg(test)]
mod security_test;
#[cfg(test)]
mod test;
