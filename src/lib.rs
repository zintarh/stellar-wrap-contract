
use soroban_sdk::{

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
            .set(&DataKey::SchemaVersion, &SCHEMA_VERSION_V3);
    }

    /// Replace the current admin with a new address.
    ///
    /// Updates the privileged admin address stored in instance storage.
    ///
    /// # Parameters
    /// - `new_admin`: The `Address` that will become the new admin.
    ///
    /// # Authorization
    /// Requires authorization from the **current** admin.
    ///
    /// # Returns
    /// Nothing on success.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - Soroban auth panics if the current admin did not authorize.
    ///
    /// # Errors
    /// This function can surface [`ContractError::NotInitialized`] (code 2).
    ///
    /// # Examples
    /// Update admin from a Soroban client.
    ///
    /// ```ignore
    /// // JavaScript / TypeScript client pattern (pseudo-code)
    /// // const client = new StellarWrapContractClient(contractId);
    /// // currentAdminAuths();
    /// // await client.update_admin({ new_admin: nextAdmin });
    /// ```
    pub fn update_admin(e: Env, new_admin: Address) {

        let current_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        current_admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &new_admin);

        e.events().publish(
            (
                symbol_short!("admin"),
                symbol_short!("updated"),
            ),
            (current_admin, new_admin),
        );
    }

    /// Mint a soulbound wrap record for a user.
    ///
    /// The backend generates a payload of
    /// `(contract_id ‖ user ‖ period ‖ archetype ‖ data_hash ‖ expiry_ledger)`,
    /// signs it with the admin Ed25519 private key, and delivers the signature to the user.
    /// The user then calls this function to claim their on-chain wrap record before
    /// `expiry_ledger` is reached.
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
    /// - `delegate`: When set, the delegate must authorize and their registered pubkey is used
    ///   for signature verification instead of the admin key.
    ///
    /// # Returns
    /// Nothing on success. Emits a `(mint, user, period) → archetype` event.
    ///
    /// # Authorization
    /// Requires authorization from `user`. When `delegate` is `Some`, that delegate must also
    /// authorize the call.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - [`ContractError::InvalidDataHash`] if `data_hash` is all-zero bytes.
    /// - [`ContractError::InvalidSignature`] if the Ed25519 signature is invalid.
    /// - [`ContractError::WrapAlreadyExists`] if a wrap for `(user, period)` already exists.
    // mint_wrap intentionally covers one complete, sequential flow (auth → verify → store → emit).
    // Splitting it would obscure the security-critical ordering of steps.
    #[allow(clippy::too_many_lines)]
    pub fn mint_wrap(
        e: Env,
        caller: Address,
        user: Address,
        period: WrapPeriod,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
    ) {
        user.require_auth();

        // 1b. Reentrancy guard in temporary storage.
        // If execution panics, the temporary TTL naturally clears stale entries.
        let guard_key = DataKey::MintGuard(user.clone());
        if e.storage().temporary().has(&guard_key) {
            panic_with_error!(e, ContractError::Unauthorized);
        }
        e.storage().temporary().set(&guard_key, &true);

        // 1c. Validate archetype format
        Self::validate_archetype(&e, &archetype);

        // 2. Verify initialization
        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        let signing_pubkey = match delegate {
            None => admin_pubkey,
            Some(ref d) => {
                d.require_auth();
                e.storage()
                    .persistent()
                    .get(&DataKey::Delegate(d.clone()))
                    .unwrap_or_else(|| panic_with_error!(e, ContractError::Unauthorized))
            }
        };

        // 3. Reject zero data_hash — all-zero bytes indicate missing or invalid data
        if data_hash == BytesN::from_array(&e, &ZERO_HASH_BYTES) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&archetype.clone().to_xdr(&e));
        payload.append(&data_hash.clone().to_xdr(&e));
        // 7. Check Duplicates & Store Record
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

        Self::persist_wrap_record(&e, symbol_short!("default"), user.clone(), period, record, archetype);


    pub fn mint_campaign_wrap(
        e: Env,
        campaign: Symbol,
        user: Address,
        period: WrapPeriod,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
        metadata: Option<String>,
    ) {
        Self::validate_period(&e, period);

        // Verify the campaign has been registered (except default)
        let default_campaign = symbol_short!("default");
        if campaign != default_campaign {
            let campaigns: soroban_sdk::Vec<Symbol> = e
                .storage()
                .instance()
                .get(&DataKey::Campaigns)
                .unwrap_or_else(|| soroban_sdk::Vec::new(&e));
            if !campaigns.contains(&campaign) {
                panic_with_error!(e, ContractError::CampaignNotFound);
            }
        } else {
            return Self::mint_wrap(e, user, period, archetype, data_hash, signature, metadata);
        }

        user.require_auth();

        let guard_key = DataKey::MintGuard(user.clone());
        if e.storage().temporary().has(&guard_key) {
            panic_with_error!(e, ContractError::Unauthorized);
        }
        e.storage().temporary().set(&guard_key, &true);

        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if data_hash == BytesN::from_array(&e, &[0u8; 32]) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        // Payload: contract_id ‖ campaign ‖ user ‖ period ‖ archetype ‖ data_hash ‖ metadata
        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&campaign.clone().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&archetype.clone().to_xdr(&e));
        payload.append(&data_hash.clone().to_xdr(&e));
        payload.append(&metadata.clone().to_xdr(&e));

        e.crypto()
            .ed25519_verify(&admin_pubkey, &payload, &signature);

        let wrap_key = DataKey::CampaignWrap(campaign.clone(), user.clone(), period);
        if e.storage().persistent().has(&wrap_key) {
            panic_with_error!(e, ContractError::WrapAlreadyExists);
        }

        let record = WrapRecord {
            timestamp: e.ledger().timestamp(),
            data_hash,
            archetype: archetype.clone(),
            period,
            image_uri: String::from_str(&e, ""),
            metadata,
        };

        Self::persist_wrap_record(&e, campaign, user.clone(), period, record, archetype);

        e.storage().temporary().remove(&guard_key);
    }

    /// Publish a merkle root for batch wrap claims in a given period (admin-only).
    ///
    /// Off-chain, build a binary merkle tree over leaves defined as:
    /// `SHA-256(XDR(user) ‖ XDR(period) ‖ XDR(archetype) ‖ XDR(data_hash))`.
    /// Internal nodes use `SHA-256(min(h1,h2) ‖ max(h1,h2))` (lexicographic order).
    pub fn set_merkle_root(e: Env, period: u64, root: BytesN<32>) {
        Self::require_not_paused(&e);

        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::MerkleRoot(period), &root);

        e.events().publish(
            root,
        );
    }

    /// Claim a wrap using a merkle proof against a published root for `period`.
    ///
    /// Requires `user.require_auth()`. Produces the same `WrapRecord` and `mint` event as
    /// `mint_wrap`, without an admin signature per claim.
    pub fn claim_wrap(
        e: Env,
        user: Address,
        period: WrapPeriod,
        archetype: Symbol,
        data_hash: BytesN<32>,
        user.require_auth();

        let guard_key = DataKey::MintGuard(user.clone());
        if e.storage().temporary().has(&guard_key) {
            panic_with_error!(e, ContractError::Unauthorized);
        }
        e.storage().temporary().set(&guard_key, &true);

        // Validate archetype format
        Self::validate_archetype(&e, &archetype);

        e.storage()
            .instance()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if data_hash == BytesN::from_array(&e, &ZERO_HASH_BYTES) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }


        let root: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::MerkleRoot(period))
            .unwrap_or_else(|| panic_with_error!(e, ContractError::MerkleRootNotSet));

        let leaf = compute_merkle_leaf(&e, &user, period, &archetype, &data_hash, &metadata);
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
    }

    fn load_wrap_record(e: &Env, user: &Address, period: u64) -> Option<WrapRecord> {
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let fmt_key = DataKey::WrapFormat(user.clone(), period);
        let fmt: u32 = e.storage().persistent().get(&fmt_key).unwrap_or(1);

        if fmt < SCHEMA_VERSION_V2 {
            return e
                .storage()
                .persistent()
                .get::<_, WrapRecordV1>(&wrap_key)
        }
        if let Some(v2) = e.storage().persistent().get::<_, WrapRecordV2>(&wrap_key) {
            return Some(Self::v2_to_v3(&v2));
        }

        None
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

    /// Set or update the image URI for an existing wrap record (admin-only).
    ///
    /// Supported URI schemes: `ipfs://`, `ar://`, `https://`.
    ///
    /// # Parameters
    /// - `user`: The address whose wrap record is being updated.
    /// - `period`: The period identifier of the record to update.
    /// - `image_uri`: URI pointing to the wrap card image (max 256 chars).
    ///
    /// # Authorization
    /// Requires authorization from the **admin**.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - [`ContractError::Unauthorized`] if the caller is not the admin.
    /// - [`ContractError::WrapNotFound`] if no wrap exists for `(user, period)`.
    /// - [`ContractError::StorageDepositExceeded`] if `image_uri` exceeds 256 characters.
    pub fn set_wrap_image(e: Env, user: Address, period: u64, image_uri: String) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();
        if image_uri.len() > 256 {
            panic_with_error!(e, ContractError::StorageDepositExceeded);
        }
        let wrap_key = DataKey::Wrap(user.clone(), period);
        let ttl_one_year = 17280 * 365;
        let schema = Self::schema_version(&e);
        // Read and write back in the correct schema format to avoid deserialization mismatch.
        if schema < SCHEMA_VERSION_V2 {
            // Schema v1: record stored as WrapRecordV1; image_uri is not persisted.
            // Confirm record exists, then no-op on storage (image not supported yet).
            let _exists: WrapRecordV1 = e
                .storage()
                .persistent()
                .get(&wrap_key)
                .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));
        } else {
            // Schema v2: read as WrapRecord, update image_uri, write back.
            let mut record: WrapRecord = e
                .storage()
                .persistent()
                .get(&wrap_key)
                .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));
            record.image_uri = image_uri.clone();
            e.storage().persistent().set(&wrap_key, &record);
            e.storage().persistent().set(&DataKey::WrapFormat(user.clone(), period), &SCHEMA_VERSION_V2);
            e.storage()
                .persistent()
                .extend_ttl(&wrap_key, ttl_one_year, ttl_one_year);
        }
        e.events().publish(
            (symbol_short!("image_set"), user, period),
            image_uri,
        );
    }
    pub fn update_wrap(
        e: Env,
        user: Address,
        period: WrapPeriod,
        new_data_hash: BytesN<32>,
        new_archetype: Symbol,
        signature: BytesN<64>,
        new_metadata: Option<String>,
    ) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));
        admin.require_auth();

        // Validate archetype format
        Self::validate_archetype(&e, &new_archetype);

        let admin_pubkey: BytesN<32> = e
            .storage()
            .instance()
            .get(&DataKey::AdminPubKey)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::NotInitialized));

        if new_data_hash == BytesN::from_array(&e, &ZERO_HASH_BYTES) {
            panic_with_error!(e, ContractError::InvalidDataHash);
        }

        let mut payload = Bytes::new(&e);
        payload.append(&e.current_contract_address().to_xdr(&e));
        payload.append(&user.clone().to_xdr(&e));
        payload.append(&period.to_xdr(&e));
        payload.append(&new_archetype.clone().to_xdr(&e));
        payload.append(&new_data_hash.clone().to_xdr(&e));

        let wrap_key = DataKey::Wrap(user.clone(), period);
        let existing: WrapRecord = Self::load_wrap_record(&e, &user, period)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

        let updated = WrapRecord {
            timestamp: existing.timestamp, // preserve original timestamp
            data_hash: new_data_hash,
            archetype: new_archetype.clone(),
            period,
            image_uri: existing.image_uri,

        e.events().publish(
            (symbol_short!("campaign"), symbol_short!("update"), campaign, user, period),
            new_archetype,
        );
    }

    /// Remove a wrap record that has passed its expiry ledger.
    ///
    /// Anyone may call this to reclaim storage for time-limited campaign wraps.
    /// Records with `expires_at == 0` (no expiry) cannot be cleaned up.
    ///
    /// # Parameters
    /// - `user`: The address whose expired wrap should be removed.
    /// - `period`: The period identifier of the wrap to clean up.
    ///
    /// # Panics
    /// - [`ContractError::WrapNotFound`] if no wrap exists for `(user, period)`.
    /// - [`ContractError::WrapNotExpired`] if the record has no expiry or is not yet expired.
    pub fn cleanup_expired_wrap(e: Env, user: Address, period: u64) {
        let record = Self::load_wrap_record(&e, &user, period)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

        if record.expires_at == 0 || (e.ledger().sequence() as u64) < record.expires_at {
            panic_with_error!(e, ContractError::WrapNotExpired);
        }

        let wrap_key = DataKey::Wrap(user.clone(), period);
        e.storage().persistent().remove(&wrap_key);

        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e.storage().persistent().get(&count_key).unwrap_or(0);
        if current_count > 0 {
            e.storage()
                .persistent()
                .set(&count_key, &(current_count - 1));
        }

    }

    /// Admin-only revocation for incorrect or fraudulent records.
    ///
    /// # Authorization
    /// Requires authorization from the **admin**.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    /// - [`ContractError::WrapNotFound`] if no wrap exists for `(user, period)`.
    pub fn revoke_wrap(e: Env, user: Address, period: u64) {
        Self::require_not_paused(&e);

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

        let existing: WrapRecord = Self::load_wrap_record(&e, &user, period)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));
        Self::decrement_archetype_count(&e, &existing.archetype);

        e.storage().persistent().remove(&wrap_key);

        let count_key = DataKey::WrapCount(user.clone());
        let current_count: u32 = e
            .storage()
            .persistent()
            .get(&count_key)
        if current_count > 0 {
            e.storage()
                .persistent()
                .set(&count_key, &(current_count - 1));
        }

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
    }

    pub fn get_campaign_wrap(
        e: Env,
        campaign: Symbol,
        user: Address,
        period: WrapPeriod,
    ) -> Option<WrapRecord> {
        if Self::user_is_opted_out(&e, &user) {
            return None;
        }
        let default_campaign = symbol_short!("default");
        if campaign == default_campaign {
            Self::load_wrap_record(&e, &user, period)
        } else {
            Self::load_campaign_wrap_record(&e, &campaign, &user, period)
        }
    }

    /// Return the total number of wrap records minted for a user (their SBT balance).
    ///
    /// # Parameters
    /// - `id`: The address to query.
    ///
    /// # Returns
    /// The number of wraps as `u32`. Returns `0` if the user has no wraps or the contract
    /// has not been initialized.
    pub fn balance_of(e: Env, id: Address) -> u32 {
        let count_key = DataKey::WrapCount(id);
        // u32 fits entirely in i128 — no truncation or sign loss is possible.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let balance = e
            .storage()
            .persistent()
            .get::<_, u32>(&count_key)
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
    pub fn verify_data(e: Env, user: Address, period: WrapPeriod, data: Bytes) -> bool {
        match Self::load_wrap_record(&e, &user, period) {
            Some(record) => record.data_hash == Self::compute_data_hash(e, data),
            None => false,
        }
    }

    /// Lightweight existence check — returns `true` if a wrap record exists for
    /// `(user, period)`, `false` otherwise.
    ///
    /// This is cheaper than `get_wrap()` for callers that only need to know whether
    /// a wrap exists, because it performs a single persistent-storage `has()` probe
    /// without deserializing the full `WrapRecord`.
    ///
    /// # Parameters
    /// - `user`: The address to check.
    /// - `period`: The period identifier to look up.
    ///
    /// # Returns
    /// `true` if a `WrapRecord` is stored for `(user, period)`, `false` otherwise.
    /// Note: this returns `true` even when the user has opted out of visibility —
    /// use `get_wrap()` when you need to respect opt-out status.
    pub fn has_wrap(e: Env, user: Address, period: u64) -> bool {
        e.storage()
            .persistent()
            .has(&DataKey::Wrap(user, period))
    }

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

    pub fn get_campaign_latest_wrap(e: Env, campaign: Symbol, user: Address) -> Option<WrapRecord> {
        if Self::user_is_opted_out(&e, &user) {
            return None;
        }
        let default_campaign = symbol_short!("default");
        if campaign == default_campaign {
            return Self::get_latest_wrap(e, user);
        }
        let latest_key = DataKey::CampaignLatestPeriod(campaign.clone(), user.clone());
        let period: u64 = e.storage().persistent().get(&latest_key)?;
        Self::load_campaign_wrap_record(&e, &campaign, &user, period)
    }

    /// Extend the TTL (time-to-live) for all persistent storage entries belonging to a user.
    ///
    /// Soroban persistent storage entries expire after their TTL lapses. This function lets
    /// anyone renew a user's wrap records so they remain accessible indefinitely.
    ///
    /// When a wrap has `expires_at` set, TTL extensions are capped so storage does not
    /// outlive the expiry ledger.
    ///
    /// # Parameters
    /// - `user`: The address whose storage entries will be extended.
    /// - `period`: The specific wrap period whose record TTL will be extended.
    pub fn extend_ttl(e: Env, user: Address, period: WrapPeriod) {
        let wrap_key = DataKey::Wrap(user.clone(), period);
    }

    /// Return the global number of wraps minted with `archetype`.
    pub fn get_archetype_count(e: Env, archetype: Symbol) -> u64 {
        e.storage()
            .persistent()
            .get(&DataKey::ArchetypeCount(archetype))
            .unwrap_or(0)
    }

    /// Return the current admin address, or `None` if the contract is not yet initialized.
    pub fn get_admin(e: Env) -> Option<Address> {
        e.storage().instance().get(&DataKey::Admin)
    }

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
        TOKEN_DECIMALS
    }

    /// Return the deployed contract version. Bump `VERSION` on every upgrade.
    pub fn version(_e: Env) -> u32 {
        VERSION
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
