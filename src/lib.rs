#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, xdr::ToXdr, Address,
    Bytes, BytesN, Env, String, Symbol,
};

mod merkle;
mod storage_types;
use merkle::{compute_merkle_leaf, verify_merkle_proof};
use storage_types::{
    ContractInfo, DataKey, WrapRecord, WrapRecordV1, SCHEMA_VERSION, SCHEMA_VERSION_V2,
};

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
    /// No merkle root has been published for this period. (code 8)
    MerkleRootNotSet = 8,
    /// The provided merkle proof does not verify against the stored root. (code 9)
    InvalidMerkleProof = 9,
    /// This `(user, period)` merkle claim has already been redeemed. (code 10)
    MerkleAlreadyClaimed = 10,
    /// `migrate()` was called with invalid version parameters. (code 11)
    InvalidMigration = 11,
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
        e.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &SCHEMA_VERSION);
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
            image_uri: String::from_str(&e, ""),
        };

        Self::persist_wrap_record(&e, user.clone(), period, record, archetype);

        e.storage().temporary().remove(&guard_key);
    }
    /// Publish a merkle root for batch wrap claims in a given period (admin-only).
    ///
    /// Off-chain, build a binary merkle tree over leaves defined as:
    /// `SHA-256(XDR(user) ‖ XDR(period) ‖ XDR(archetype) ‖ XDR(data_hash))`.
    /// Internal nodes use `SHA-256(min(h1,h2) ‖ max(h1,h2))` (lexicographic order).
    pub fn set_merkle_root(e: Env, period: u64, root: BytesN<32>) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::MerkleRoot(period), &root);

        e.events()
            .publish((symbol_short!("merkle"), symbol_short!("root"), period), root);
    }

    /// Claim a wrap using a merkle proof against a published root for `period`.
    ///
    /// Requires `user.require_auth()`. Produces the same `WrapRecord` and `mint` event as
    /// `mint_wrap`, without an admin signature per claim.
    pub fn claim_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        proof: soroban_sdk::Vec<BytesN<32>>,
    ) {
        user.require_auth();

        let guard_key = DataKey::MintGuard(user.clone());
        if e.storage().temporary().has(&guard_key) {
            panic_with_error!(e, ContractError::Unauthorized);
        }
        e.storage().temporary().set(&guard_key, &true);

        e.storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if data_hash == BytesN::from_array(&e, &[0u8; 32]) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        let root: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::MerkleRoot(period))
            .unwrap_or_else(|| panic_with_error!(e, ContractError::MerkleRootNotSet));

        let leaf = compute_merkle_leaf(&e, &user, period, &archetype, &data_hash);
        if !verify_merkle_proof(&e, &root, &leaf, &proof) {
            panic_with_error!(e, ContractError::InvalidMerkleProof);
        }

        let claim_key = DataKey::MerkleClaimed(user.clone(), period);
        if e.storage().persistent().has(&claim_key) {
            panic_with_error!(e, ContractError::MerkleAlreadyClaimed);
        }

        let wrap_key = DataKey::Wrap(user.clone(), period);
        if e.storage().persistent().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapAlreadyExists);
        }

        let record = WrapRecord {
            timestamp: e.ledger().timestamp(),
            data_hash,
            archetype: archetype.clone(),
            period,
            image_uri: String::from_str(&e, ""),
        };

        let ttl_one_year = 17280 * 365;
        e.storage().persistent().set(&claim_key, &true);
        e.storage()
            .persistent()
            .extend_ttl(&claim_key, ttl_one_year, ttl_one_year);

        Self::persist_wrap_record(&e, user.clone(), period, record, archetype);

        e.storage().temporary().remove(&guard_key);
    }

    /// Run a one-shot schema migration after a WASM upgrade (admin-only).
    ///
    /// Records are lazily upgraded on read via `get_wrap`. This function advances
    /// `SchemaVersion` and may only be called once per version transition.
    pub fn migrate(e: Env, from_version: u32, to_version: u32) -> u32 {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        let current: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(SCHEMA_VERSION);

        if current != from_version || to_version != from_version + 1 {
            panic_with_error!(e, ContractError::InvalidMigration);
        }

        e.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &to_version);

        e.events().publish(
            (symbol_short!("schema"), symbol_short!("migrat")),
            (from_version, to_version),
        );

        to_version
    }

    /// Return the current storage schema version.
    pub fn get_schema_version(e: Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(SCHEMA_VERSION)
    }

    /// Hide all wrap records for `user` from public queries (`get_wrap`, `get_latest_wrap`).
    pub fn opt_out(e: Env, user: Address) {
        user.require_auth();
        e.storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        let ttl_one_year = 17280 * 365;
        e.storage()
            .persistent()
            .set(&DataKey::UserOptOut(user.clone()), &true);
        e.storage().persistent().extend_ttl(
            &DataKey::UserOptOut(user.clone()),
            ttl_one_year,
            ttl_one_year,
        );

        e.events()
            .publish((symbol_short!("opt_out"), user), true);
    }

    /// Re-enable public visibility of wrap records for `user`.
    pub fn opt_in(e: Env, user: Address) {
        user.require_auth();
        e.storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        e.storage().persistent().remove(&DataKey::UserOptOut(user.clone()));

        e.events()
            .publish((symbol_short!("opt_in"), user), true);
    }

    /// Return whether `user` has opted out of public wrap visibility.
    pub fn is_opted_out(e: Env, user: Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::UserOptOut(user))
            .unwrap_or(false)
    }

    fn schema_version(e: &Env) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(SCHEMA_VERSION)
    }

    fn v1_to_v2(e: &Env, v1: &WrapRecordV1) -> WrapRecord {
        WrapRecord {
            timestamp: v1.timestamp,
            data_hash: v1.data_hash.clone(),
            archetype: v1.archetype.clone(),
            period: v1.period,
            image_uri: String::from_str(e, ""),
        }
    }

    fn migrate_v1_record(e: &Env, user: &Address, period: u64, v1: WrapRecordV1) -> WrapRecord {
        let migrated = Self::v1_to_v2(e, &v1);
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let ttl_one_year = 17280 * 365;
        e.storage().persistent().set(&wrap_key, &migrated);
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, ttl_one_year, ttl_one_year);
        e.events().publish(
            (symbol_short!("migrat"), user.clone(), period),
            v1.archetype,
        );
        migrated
    }

    fn persist_wrap_record(
        e: &Env,
        user: Address,
        period: u64,
        record: WrapRecord,
        archetype: Symbol,
    ) {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let ttl_one_year = 17280 * 365;
        if Self::schema_version(e) < SCHEMA_VERSION_V2 {
            let v1 = WrapRecordV1 {
                timestamp: record.timestamp,
                data_hash: record.data_hash,
                archetype: record.archetype.clone(),
                period: record.period,
            };
            e.storage().persistent().set(&wrap_key, &v1);
        } else {
            e.storage().persistent().set(&wrap_key, &record);
        }
        e.storage()
            .persistent()
            .extend_ttl(&wrap_key, ttl_one_year, ttl_one_year);

        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
        e.storage()
            .persistent()
            .set(&count_key, &(current_count + 1));
        e.storage()
            .persistent()
            .extend_ttl(&count_key, ttl_one_year, ttl_one_year);

        let latest_key = DataKey::LatestPeriod(user.clone());
        let current_latest: u64 = e.storage().persistent().get(&latest_key).unwrap_or(0);
        if period > current_latest {
            e.storage().persistent().set(&latest_key, &period);
            e.storage()
                .persistent()
                .extend_ttl(&latest_key, ttl_one_year, ttl_one_year);
        }

        e.events()
            .publish((symbol_short!("mint"), user, period), archetype);
    }

    fn load_wrap_record(e: &Env, user: &Address, period: u64) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let schema = Self::schema_version(e);

        if schema < SCHEMA_VERSION_V2 {
            return e
                .storage()
                .persistent()
                .get::<_, WrapRecordV1>(&wrap_key)
                .map(|v1| Self::v1_to_v2(e, &v1));
        }

        // After migration, unread v1 records must be tried before v2 deserialization.
        if let Some(v1) = e.storage().persistent().get::<_, WrapRecordV1>(&wrap_key) {
            return Some(Self::migrate_v1_record(e, user, period, v1));
        }

        e.storage().persistent().get::<_, WrapRecord>(&wrap_key)
    }

    fn user_is_opted_out(e: &Env, user: &Address) -> bool {
        e.storage()
            .persistent()
            .get(&DataKey::UserOptOut(user.clone()))
            .unwrap_or(false)
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
        let existing: WrapRecord = Self::load_wrap_record(&e, &user, period)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

        let updated = WrapRecord {
            timestamp: existing.timestamp, // preserve original timestamp
            data_hash: new_data_hash,
            archetype: new_archetype.clone(),
            period,
            image_uri: existing.image_uri,
        };

        let ttl_one_year = 17280 * 365;
        if Self::schema_version(&e) < SCHEMA_VERSION_V2 {
            let v1 = WrapRecordV1 {
                timestamp: updated.timestamp,
                data_hash: updated.data_hash.clone(),
                archetype: updated.archetype.clone(),
                period: updated.period,
            };
            e.storage().persistent().set(&wrap_key, &v1);
        } else {
            e.storage().persistent().set(&wrap_key, &updated);
        }
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
        if Self::user_is_opted_out(&e, &user) {
            return None;
        }
        Self::load_wrap_record(&e, &user, period)
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
        match Self::load_wrap_record(&e, &user, period) {
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
        if Self::user_is_opted_out(&e, &user) {
            return None;
        }
        let latest_key = DataKey::LatestPeriod(user.clone());
        let period: u64 = e.storage().persistent().get(&latest_key)?;
        Self::load_wrap_record(&e, &user, period)
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

    /// Return contract-level metadata useful for explorers and indexers.
    pub fn contract_info(e: Env) -> ContractInfo {
        ContractInfo {
            name: String::from_str(&e, "Stellar Wrap Registry"),
            version: String::from_str(&e, "0.1.0"),
            repo: String::from_str(&e, "https://github.com/zintarh/stellar-wrap-contract"),
            description: String::from_str(&e, "Soulbound token registry for Stellar Wrap"),
        }
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
