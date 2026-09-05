//! # Stellar Wrap Registry
//!
//! commitments on-chain. Each wrap binds a user address, a period
//! (represented as a `u64` in `YYYYMM` format), an archetype label, and a SHA-256
//! data hash into an immutable record.
//!
//! ### Period Format Contract
//! - **Type**: Unsigned 64-bit integer (`u64`).
//! - **Canonical Format**: `YYYYMM` (e.g. `202512` for December 2025).
//! - **Validation**: Enforced on-chain to have year between `2024` and `2100`, and month between `01` and `12`.
//! - **Non-Monthly Periods**: Not natively supported by the validation rules. Integrations must map non-monthly periods (weekly, daily, quarterly) to a valid `YYYYMM` value.
//!
//! ## Security
//!
//! Minting requires an Ed25519 signature from the configured admin key
//! over the full payload (contract ID, user, period, archetype, data hash).
//! This prevents unauthorized wraps even if the caller contract is
//! compromised. The admin address controls the public-key rotation.

#![no_std]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

#[cfg(any(test, feature = "testutils"))]
extern crate std;

use soroban_sdk::{
    contract, contractimpl, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

mod admin;
mod alias;
mod bridge;
mod burn;
mod constants;
mod errors;
mod events;
mod governance;
mod merkle;
mod mint;
mod optout;
mod oracle;
mod queries;
mod revoke;
mod remove_wrap;
pub mod signature;
mod stake;
mod storage_accounting;
mod storage_types;
mod timelock;
mod token;
mod transfer;
mod ttl;
mod wrap_record_helpers;

pub use errors::ContractError;
pub use mint::{validate_period, CURRENT_PAYLOAD_VERSION, MAX_PERIOD_YEAR, MIN_PERIOD_YEAR};
pub use oracle::DataHashOracle;
pub use storage_types::{
    AdminProposal, BatchWrapItem, ContractHealth, DataKey, InboundBridgeRecord, InvariantReport,
    OutboundBridgeRequest, ProposalStatus, StakeConfig, StakeRecord, TimelockAction,
    TimelockOperation, TransferFeeConfig, WrapLifecycleFSM, WrapRecord, WrapState,
};
pub use token::TokenInterface;

const MAX_WRAP_DESCRIPTION_LEN: u32 = 256;
const MAX_WRAP_IMAGE_URL_LEN: u32 = 2048;

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl StellarWrapContract {
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
        admin::initialize(e, admin, admin_pubkey);
    }

    pub fn update_admin(e: Env, new_admin: Address) {
        admin::update_admin(e, new_admin);
    }

    /// Configures the token-denominated fee charged by `transfer_wrap`.
    ///
    /// Only the current admin may update the configuration. An amount of zero
    /// enables fee-free transfers without removing the configured token and
    /// recipient.
    pub fn set_transfer_fee(e: Env, token: Address, recipient: Address, amount: i128) {
        admin::set_transfer_fee(e, token, recipient, amount);
    }

    /// Admin-only: remove the configured transfer fee, returning the contract
    /// to the unconfigured state where transfers are free by default.
    pub fn clear_transfer_fee(e: Env) {
        admin::clear_transfer_fee(e);
    }

    pub fn update_admin_pubkey(e: Env, new_pubkey: BytesN<32>) {
        admin::update_admin_pubkey(e, new_pubkey);
    }

    pub fn pause(e: Env) {
        admin::set_pause(e, true);
    }

    pub fn unpause(e: Env) {
        admin::set_pause(e, false);
    }

    pub fn is_paused(e: Env) -> bool {
        admin::is_paused(&e)
    }

    /// Records that the storage migration `version` has been applied.
    /// Admin-only, and each version can only be applied once.
    pub fn migrate(e: Env, version: u32) {
        admin::migrate(e, version);
    }

    pub fn migration_version(e: Env) -> u32 {
        admin::migration_version(&e)
    }

    pub fn upgrade(e: Env, new_wasm_hash: BytesN<32>) {
        admin::upgrade(e, new_wasm_hash);
    }

    pub fn propose_admin(e: Env, new_admin: Address) {
        admin::propose_admin(e, new_admin);
    }

    pub fn accept_admin(e: Env) {
        admin::accept_admin(e);
    }

    pub fn cancel_proposed_admin(e: Env) {
        admin::cancel_proposed_admin(e);
    }

    pub fn get_pending_admin(e: Env) -> Option<Address> {
        admin::get_pending_admin(e)
    }

    pub fn set_name(e: Env, name: String) {
        admin::set_name(e, name);
    }

    pub fn set_symbol(e: Env, symbol: String) {
        admin::set_symbol(e, symbol);
    }

    pub fn mint_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        payload_version: u32,
        signature: BytesN<64>,
    ) {
        mint::mint_wrap(
            e,
            user,
            period,
            archetype,
            data_hash,
            payload_version,
            signature,
        );
    }

    pub fn mint_wrap_batch(
        e: Env,
        items: soroban_sdk::Vec<storage_types::BatchWrapItem>,
        aggregated_signature: Option<BytesN<64>>,
    ) {
        mint::mint_wrap_batch(e, items, aggregated_signature);
    }
    /// Updates the optional display metadata for an existing wrap.
    ///
    /// Only the wrap owner (`user`) may update the metadata. Both fields are
    /// optional: pass `None` to clear, or `Some(...)` to set. The description
    /// is limited to 256 bytes and the image URL to 2048 bytes.
    ///
    /// Emits a `set_wrap_metadata` event with the resulting metadata.
    pub fn set_wrap_metadata(
        e: Env,
        user: Address,
        period: u64,
        description: Option<String>,
        image_url: Option<String>,
    ) {
        wrap_record_helpers::set_wrap_metadata(e, user, period, description, image_url);
    }

    /// Transfers one wrap record and atomically charges the configured fee.
    ///
    /// The current owner (`from`) must authorize the invocation. The record is
    /// moved only if fee payment succeeds; any token-contract failure rolls the
    /// entire invocation back.
    pub fn transfer_wrap(e: Env, from: Address, to: Address, period: u64) {
        transfer::transfer_wrap(e, from, to, period);
    }

    /// Backfills the ownership-period index for records minted before transfer
    /// support was deployed. Admin-only and callable once per user.
    pub fn backfill_wrap_periods(e: Env, user: Address, periods: Vec<u64>) {
        transfer::backfill_wrap_periods(e, user, periods);
    }

    pub fn transition_wrap_state(e: Env, user: Address, period: u64, next_state: WrapState) {
        mint::transition_wrap_state(e, user, period, next_state);
    }

    /// Return the configured expiration duration (seconds) for unverified wraps.
    /// Defaults to 7 days when unset.
    pub fn expiration_duration(e: Env) -> u64 {
        mint::get_expiration_duration(&e)
    }

    /// Admin-only: set the expiration duration (seconds) for unverified wraps.
    pub fn set_expiration_duration(e: Env, duration: u64) {
        mint::set_expiration_duration(&e, duration);
    }

    /// Expire an unverified wrap whose deadline has passed. Callable by anyone.
    pub fn expire_wrap(e: Env, user: Address, period: u64) {
        mint::expire_wrap(e, user, period);
    }

    pub fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
        queries::get_wrap(e, user, period)
    }

    /// Returns the mint timestamp for a known user-period.
    /// The timestamp reflects ledger time, not wall-clock time.
    /// Returns `None` if no mint has occurred for the given user-period.
    pub fn get_mint_timestamp(e: Env, user: Address, period: u64) -> Option<u64> {
        queries::get_mint_timestamp(e, user, period)
    }

    /// Returns the ledger timestamp of the user's most recent state change via
    /// a successful mint or revoke, or `None` if the user has never minted or
    /// had a wrap revoked. The value is monotonic (non-decreasing) per user.
    pub fn get_last_updated(e: Env, user: Address) -> Option<u64> {
        queries::get_last_updated(e, user)
    }

    /// Returns the number of wraps currently live on this contract.
    ///
    /// This counter is incremented by `mint_wrap`, `mint_wrap_batch`, and
    /// `bridge_wrap_in`, and decremented by `revoke_wrap` and `burn_wrap`.
    pub fn total_wrap_count(e: Env) -> u32 {
        queries::total_wrap_count(e)
    }

    pub fn verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool {
        queries::verify_data(e, user, period, data)
    }

    /// Asks an external oracle contract whether `data_hash` is recognized.
    ///
    /// The oracle must expose `verify_data_hash(BytesN<32>) -> bool`.
    /// Oracle invocation and ABI errors propagate to the caller.
    pub fn verify_with_oracle(e: Env, oracle: Address, data_hash: BytesN<32>) -> bool {
        oracle::verify_data_hash(&e, &oracle, &data_hash)
    }

    pub fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
        queries::get_latest_wrap(e, user)
    }

    pub fn get_wraps(
        e: Env,
        user: Address,
        start: u32,
        limit: u32,
    ) -> soroban_sdk::Vec<WrapRecord> {
        queries::get_wraps(e, user, start, limit)
    }

    /// Read-only check to verify the internal consistency of a user's wrap state.
    /// Returns an `InvariantReport` containing boolean flags for each invariant and observed values.
    pub fn check_user_invariants(e: Env, user: Address) -> InvariantReport {
        queries::check_user_invariants(e, user)
    }

    /// Returns every wrap record owned by `user` in a single call.
    ///
    /// This is a convenience wrapper around [`Self::get_wraps`] that fetches all
    /// records without pagination. It is intended for bounded queries of at most
    /// 200 records. For users with more wraps, prefer the paginated
    /// [`Self::get_wraps`] to stay within Soroban resource limits.
    ///
    /// **Note:** The 200-record bound is not yet enforced at runtime; this
    /// function currently still requests all records in one call.
    pub fn get_all_wraps_for_user(e: Env, user: Address) -> soroban_sdk::Vec<WrapRecord> {
        queries::get_all_wraps_for_user(e, user)
    }

    /// Extend the TTL (time-to-live) for all persistent storage entries belonging to a user.
    ///
    /// Soroban persistent storage entries expire after their TTL lapses. This function lets
    /// anyone renew a user's wrap records so they remain accessible indefinitely.
    ///
    /// # TTL Lifecycle
    ///
    /// All persistent storage entries (wraps, balance, latest-period marker) are stored with
    /// a TTL of ~1 year (17280 × 365 ledgers) at creation time.
    ///
    /// **Automatic renewal (metadata only):** When `mint_wrap` is called, the `WrapCount`
    /// and `LatestPeriod` metadata keys are automatically extended by another ~1 year.
    /// This keeps the user's balance-of and latest-wrap lookup alive for active users
    /// without any manual intervention.
    ///
    /// **Manual renewal (individual wraps):** Historical wrap records for specific
    /// `(user, period)` pairs are **not** automatically extended on new mints.
    /// Anyone can call this `extend_ttl` function to renew a specific wrap record.
    ///
    /// **Bulk renewal (admin):** The `renew_all_ttls` function allows the admin to
    /// extend the TTL of all metadata keys for a user. Full wrap-enumeration renewal
    /// requires period tracking (see Issue #90).
    ///
    /// **Expiry risk:** Without periodic renewal, the first wraps of an active multi-year
    /// user could expire after ~1 year, even though the user is still participating.
    /// Off-chain bots or the admin should call `extend_ttl` for historical periods
    /// of active users to prevent data loss.
    ///
    /// # Parameters
    /// - `user`: The address whose storage entries will be extended.
    /// - `period`: The specific wrap period whose record TTL will be extended.
    ///
    /// # Security (Issue #124)
    /// This function is intentionally callable by anyone with no `require_auth`,
    /// so off-chain renewal bots can keep active users' data alive without
    /// needing a signing key. To stop that openness from being abused to keep
    /// logically-dead records around forever (defeating the expiry mechanism
    /// in #95), the individual wrap record's TTL is only extended while it is
    /// in a non-terminal `WrapState` (`Draft`, `Pending`, `Active`, `Bridged`).
    /// Wraps that have transitioned to `Cancelled`, `Expired`, or `Archived`
    /// are skipped so their ledger entries can be naturally archived instead
    /// of being kept alive indefinitely at no cost to the caller beyond the
    /// call's own resource fee.
    pub fn extend_ttl(e: Env, user: Address, period: u64) {
        ttl::extend_ttl(e, user, period);
    }

    #[cfg(any())]
    #[allow(dead_code)]
    fn _old_extend_ttl(e: Env, user: Address, period: u64) {
        ttl::extend_ttl(e, user, period);
    }

    /// Admin-only function to extend TTL for all metadata keys associated with a user.
    ///
    /// This extends the TTL (time-to-live) for `WrapCount` and `LatestPeriod` storage
    /// entries, keeping the user's balance and latest-period data alive. It also extends
    /// the contract instance TTL. Individual historical wrap records are **not** extended
    /// — full per-wrap renewal requires period enumeration, tracked as Issue #90.
    ///
    /// # Motivation
    ///
    /// Active users who mint new wraps periodically will have their metadata keys
    /// automatically renewed by `mint_wrap`. However, if there is a long gap between
    /// mints, the metadata keys could expire. This function lets the admin proactively
    /// renew a user's metadata without requiring a new mint.
    ///
    /// # Parameters
    /// - `user`: The address whose metadata storage TTLs will be extended.
    ///
    /// # Authorization
    /// Requires authorization from the **admin**.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the contract has not been initialized.
    pub fn renew_all_ttls(e: Env, user: Address) {
        ttl::renew_all_ttls(e, user);
    }

    #[cfg(any())]
    #[allow(dead_code)]
    fn _old_renew_all_ttls(e: Env, user: Address) {
        ttl::renew_all_ttls(e, user);
    }

    /// Return the current admin address, or `None` if the contract is not yet initialized.
    pub fn get_admin(e: Env) -> Option<Address> {
        queries::get_admin(e)
    }

    /// Return the Ed25519 admin public key, or `None` before initialization.
    ///
    /// # Privacy / ops
    /// Exposes only the public verification key used for mint signatures. Useful
    /// for operators and clients to confirm key rotation without reading raw
    /// storage. Does not expose private key material.
    pub fn get_admin_pubkey(e: Env) -> Option<BytesN<32>> {
        queries::get_admin_pubkey(e)
    }

    /// Return the contract semantic version (`MAJOR.MINOR.PATCH`).
    ///
    /// Bump this string whenever a WASM upgrade changes the public interface or
    /// storage semantics so clients can detect the live contract revision.
    pub fn version(e: Env) -> String {
        queries::version(e)
    }

    /// Return whether a wrap exists for `(user, period)` without fetching the record.
    ///
    /// Prefer this over `Self::get_wrap` when only a boolean check is needed.
    pub fn has_wrap(e: Env, user: Address, period: u64) -> bool {
        queries::has_wrap(e, user, period)
    }

    pub fn get_transfer_fee(e: Env) -> Option<TransferFeeConfig> {
        queries::get_transfer_fee(e)
    }

    pub fn health(e: Env) -> ContractHealth {
        queries::health(e)
    }

    /// Set or update the caller's alias hash.
    ///
    /// Only the `user` themselves can call this — `require_auth` is enforced
    /// inside the alias module. The hash is stored as opaque 32-byte data so
    /// no raw personal information ever touches the chain.
    /// Does not require the contract to be initialized.
    pub fn set_alias_hash(e: Env, user: Address, alias_hash: BytesN<32>) {
        alias::set_alias_hash(e, user, alias_hash);
    }

    /// Return the alias hash for `user`, or `None` if one has not been set.
    pub fn get_alias_hash(e: Env, user: Address) -> Option<BytesN<32>> {
        alias::get_alias_hash(e, user)
    }

    /// Set the caller's opt-out flag, preventing any future wraps from being
    /// minted for them. Only the user themselves can call this.
    pub fn opt_out(e: Env, user: Address) {
        optout::opt_out(e, user);
    }

    #[cfg(any())]
    #[allow(dead_code)]
    fn _old_opt_out(e: Env, user: Address) {
        optout::opt_out(e, user);
    }

    /// Clear the caller's opt-out flag, allowing future wraps to be minted for
    /// them again. Only the user themselves can call this.
    pub fn opt_in(e: Env, user: Address) {
        optout::opt_in(e, user);
    }

    #[cfg(any())]
    #[allow(dead_code)]
    fn _old_opt_in(e: Env, user: Address) {
        optout::opt_in(e, user);
    }

    /// Returns `true` if the user has opted out of future mints.
    pub fn is_opted_out(e: Env, user: Address) -> bool {
        optout::is_opted_out(e, user)
    }

    #[cfg(any())]
    #[allow(dead_code)]
    fn _old_is_opted_out(e: Env, user: Address) -> bool {
        optout::is_opted_out(&e, &user)
    }

    /// Return the current contract version number.
    ///
    /// The version starts at `0` and is incremented automatically each time
    /// the admin calls `upgrade` to replace the contract WASM. This provides
    /// an on-chain audit trail of upgrade events.
    pub fn contract_version(e: Env) -> u32 {
        queries::contract_version(e)
    }

    pub fn revoke_wrap(e: Env, user: Address, period: u64, reason_hash: BytesN<32>) {
        revoke::revoke_wrap(e, user, period, reason_hash);
    }

    pub fn burn_wrap(e: Env, user: Address, period: u64) {
        burn::burn_wrap(e, user, period);
    }

    /// Returns the total number of wraps that have been revoked globally.
    ///
    /// Note: This counter only tracks revocations (via `revoke_wrap`).
    /// It is unaffected by wrap burns (via `burn_wrap`).
    pub fn total_revoked(e: Env) -> u64 {
        queries::total_revoked(e)
    }

    /// Returns estimated current persistent storage bytes used by the contract.
    pub fn storage_bytes(e: Env) -> u64 {
        storage_accounting::get_storage_bytes(&e)
    }

    /// Returns the computed current fee according to the on-chain params.
    pub fn current_fee(e: Env) -> i128 {
        storage_accounting::compute_current_fee(&e)
    }

    /// Admin: set fee params.
    pub fn set_fee_params(e: Env, params: storage_types::FeeParams) {
        storage_accounting::set_fee_params(&e, params);
    }

    /// View: fee params
    pub fn fee_params(e: Env) -> storage_types::FeeParams {
        storage_accounting::get_fee_params(&e)
    }

    // ---------------------------------------------------------------------
    // Off-chain whitelisting (merkle)
    // ---------------------------------------------------------------------

    /// Admin: publish the merkle root of the off-chain whitelist.
    ///
    /// Only the 32-byte root is stored; the member list stays off-chain. See
    /// `docs/whitelist-merkle.md` for the leaf encoding and tree layout.
    pub fn set_whitelist_root(e: Env, root: BytesN<32>) {
        merkle::set_whitelist_root(e, root);
    }

    /// Admin: remove the whitelist root, disabling whitelist checks.
    pub fn clear_whitelist_root(e: Env) {
        merkle::clear_whitelist_root(e);
    }

    /// Return the published whitelist root, or `None` if none is set.
    pub fn get_whitelist_root(e: Env) -> Option<BytesN<32>> {
        merkle::get_whitelist_root(&e)
    }

    /// Return the whitelist leaf hash for `user`, so off-chain tooling can
    /// verify it builds identical leaves.
    pub fn whitelist_leaf(e: Env, user: Address) -> BytesN<32> {
        merkle::compute_whitelist_leaf(&e, &user)
    }

    /// Verify that `user` belongs to the published whitelist.
    ///
    /// `proof` is the list of sibling hashes ordered from the leaf's sibling up
    /// to the root. Returns `false` for a non-matching proof.
    ///
    /// # Panics
    /// - [`ContractError::MerkleRootNotSet`] if no root has been published.
    pub fn verify_whitelist(e: Env, user: Address, proof: soroban_sdk::Vec<BytesN<32>>) -> bool {
        merkle::verify_whitelist(e, user, proof)
    }

    // ---------------------------------------------------------------------
    // Timelock controller
    // ---------------------------------------------------------------------

    /// Admin: enable the timelock with `delay_seconds` (1 hour – 30 days).
    ///
    /// One-way switch. Afterwards, admin handover, key rotation, WASM upgrades
    /// and whitelist-root changes must go through `timelock_schedule` +
    /// `timelock_execute`. See `docs/timelock.md`.
    pub fn enable_timelock(e: Env, delay_seconds: u64) {
        timelock::enable(e, delay_seconds);
    }

    /// Return the configured timelock delay in seconds, or `None` if disabled.
    pub fn timelock_delay(e: Env) -> Option<u64> {
        timelock::delay(&e)
    }

    /// Admin: queue `action` for execution after the timelock delay.
    /// Returns the operation id.
    pub fn timelock_schedule(e: Env, action: TimelockAction) -> BytesN<32> {
        timelock::schedule(e, action)
    }

    /// Admin: apply a queued operation whose ETA has been reached.
    pub fn timelock_execute(e: Env, id: BytesN<32>) {
        timelock::execute(e, id);
    }

    /// Admin: discard a queued operation before it executes.
    pub fn timelock_cancel(e: Env, id: BytesN<32>) {
        timelock::cancel(e, id);
    }

    /// Return a queued operation by id, or `None` if it is not queued.
    pub fn timelock_operation(e: Env, id: BytesN<32>) -> Option<TimelockOperation> {
        timelock::get_operation(&e, id)
    }

    /// Return the ids of all queued operations.
    pub fn timelock_pending(e: Env) -> soroban_sdk::Vec<BytesN<32>> {
        timelock::pending_operations(&e)
    }

    /// Compute the deterministic operation id for `action` without scheduling
    /// it, so callers can pre-compute the id they will need to execute.
    pub fn timelock_operation_id(e: Env, action: TimelockAction) -> BytesN<32> {
        timelock::operation_id(&e, &action)
    }

    /// Admin: Set the sole bridge relayer address used to authorize bridge refunds.
    pub fn set_bridge_relayer(e: Env, relayer: Address) {
        bridge::set_bridge_relayer(&e, relayer);
    }

    /// Admin: Set the cross-chain token bridge relayers for a given chain.
    pub fn set_bridge_relayers(
        e: Env,
        chain_id: u32,
        relayers: soroban_sdk::Vec<BytesN<32>>,
        threshold: u32,
    ) {
        bridge::set_bridge_relayers(&e, chain_id, relayers, threshold);
    }

    /// Admin: Set the legacy single bridge relayer address (for refund auth).
    pub fn set_bridge_relayer(e: Env, relayer: Address) {
        bridge::set_bridge_relayer(&e, relayer);
    }

    /// Returns the configured cross-chain token bridge relayers for a given chain.
    pub fn get_bridge_relayers(e: Env, chain_id: u32) -> Option<storage_types::BridgeRelayerSet> {
        bridge::get_bridge_relayers(&e, chain_id)
    }

    /// Admin: Set enabled status for a destination/source cross-chain network chain ID.
    pub fn set_chain_status(e: Env, chain_id: u32, enabled: bool) {
        bridge::set_chain_status(&e, chain_id, enabled);
    }

    /// View: Check if a cross-chain network chain ID is enabled.
    pub fn is_chain_supported(e: Env, chain_id: u32) -> bool {
        bridge::is_chain_supported(&e, chain_id)
    }

    /// Initiate an outbound cross-chain wrap bridge transfer.
    pub fn bridge_wrap_out(
        e: Env,
        user: Address,
        destination_chain: u32,
        recipient_address: Bytes,
        period: u64,
    ) -> u64 {
        bridge::bridge_wrap_out(e, user, destination_chain, recipient_address, period)
    }

    /// Relayer-authorized refund for an outbound bridge request rejected by
    /// the destination chain. Restores the locked wrap to `Active`.
    pub fn bridge_wrap_refund(e: Env, outbound_nonce: u64) {
        bridge::bridge_wrap_refund(e, outbound_nonce);
    }

    /// Fulfill an inbound cross-chain wrap bridge transfer from external chain.
    #[allow(clippy::too_many_arguments)]
    pub fn bridge_wrap_in(
        e: Env,
        source_chain: u32,
        source_nonce: u64,
        recipient: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signatures: soroban_sdk::Vec<BytesN<64>>,
    ) {
        bridge::bridge_wrap_in(
            e,
            source_chain,
            source_nonce,
            recipient,
            period,
            archetype,
            data_hash,
            signatures,
        );
    }

    /// View: Fetch an outbound bridge request record by nonce.
    pub fn get_outbound_bridge_request(e: Env, nonce: u64) -> Option<OutboundBridgeRequest> {
        bridge::get_outbound_bridge_request(&e, nonce)
    }

    /// View: Fetch an inbound bridge record by source chain and source nonce.
    pub fn get_inbound_bridge_record(
        e: Env,
        source_chain: u32,
        source_nonce: u64,
    ) -> Option<InboundBridgeRecord> {
        bridge::get_inbound_bridge_record(&e, source_chain, source_nonce)
    }

    /// View: Check if an inbound cross-chain nonce was already processed.
    pub fn is_inbound_nonce_processed(e: Env, source_chain: u32, source_nonce: u64) -> bool {
        bridge::is_inbound_nonce_processed(&e, source_chain, source_nonce)
    }

    /// View: Get current total outbound bridge nonce count.
    pub fn get_outbound_nonce(e: Env) -> u64 {
        bridge::get_outbound_nonce(&e)
    }

    /// DAO Governance: Create a proposal to update the contract admin.
    pub fn create_admin_proposal(
        e: Env,
        proposer: Address,
        proposed_admin: Address,
        duration_seconds: u64,
    ) -> u64 {
        governance::create_admin_proposal(e, proposer, proposed_admin, duration_seconds)
    }

    /// DAO Governance: Cast a vote on an active admin proposal.
    pub fn vote_admin_proposal(e: Env, voter: Address, proposal_id: u64, support: bool) {
        governance::vote_admin_proposal(e, voter, proposal_id, support);
    }

    /// DAO Governance: Execute a proposal after voting period has ended.
    pub fn execute_admin_proposal(e: Env, proposal_id: u64) {
        governance::execute_admin_proposal(e, proposal_id);
    }

    /// DAO Governance: Cancel an active proposal. Proposer or current admin can cancel.
    pub fn cancel_admin_proposal(e: Env, caller: Address, proposal_id: u64) {
        governance::cancel_admin_proposal(e, caller, proposal_id);
    }

    /// DAO Governance: Query proposal details by ID.
    pub fn get_admin_proposal(e: Env, proposal_id: u64) -> Option<AdminProposal> {
        governance::get_admin_proposal(&e, proposal_id)
    }

    /// DAO Governance: Query vote cast by a specific voter on a proposal.
    pub fn get_admin_proposal_vote(e: Env, proposal_id: u64, voter: Address) -> Option<bool> {
        governance::get_admin_proposal_vote(&e, proposal_id, voter)
    }

    /// DAO Governance: Query total proposal count.
    pub fn get_admin_proposal_count(e: Env) -> u64 {
        governance::get_admin_proposal_count(&e)
    }

    // ── Staking ──────────────────────────────────────────────────────────

    /// Stake tokens to earn wrap fee priority.
    ///
    /// The amount must be at least the configured `min_stake`. Staking more
    /// tokens increases the user's priority, which translates to a fee
    /// discount (in basis points) when minting wraps.
    ///
    /// # Authorization
    /// `user` must authorize the call.
    pub fn stake(e: Env, user: Address, amount: i128) {
        stake::stake(e, user, amount);
    }

    /// Initiate the unstaking process.
    ///
    /// After calling this, the user must wait for the cooldown period
    /// (configured in `StakeConfig`) before they can withdraw their stake
    /// via `withdraw_stake`.
    ///
    /// While unstaking is in progress, the user receives no fee priority.
    ///
    /// # Authorization
    /// `user` must authorize the call.
    pub fn unstake(e: Env, user: Address) {
        stake::unstake(e, user);
    }

    /// Complete the unstaking process and withdraw staked funds.
    ///
    /// Can only be called after the cooldown period has elapsed since
    /// `unstake` was called.
    ///
    /// # Authorization
    /// `user` must authorize the call.
    pub fn withdraw_stake(e: Env, user: Address) {
        stake::withdraw_stake(e, user);
    }

    /// Return the staking record for `user`, or `None` if they have not staked.
    pub fn get_stake(e: Env, user: Address) -> Option<StakeRecord> {
        stake::get_stake(&e, user)
    }

    /// Return the fee-discount priority (in basis points) for `user`.
    ///
    /// Returns 0 if the user has no active stake.
    pub fn get_stake_priority(e: Env, user: Address) -> u32 {
        stake::get_stake_priority(&e, user)
    }

    /// Return the total amount staked across all users.
    pub fn total_staked(e: Env) -> i128 {
        stake::get_total_staked(&e)
    }

    /// Admin: set the staking configuration.
    ///
    /// # Panics
    /// - If `min_stake == 0` or `cooldown_seconds == 0`
    /// - If `max_priority_bps > 10_000`
    pub fn set_stake_config(e: Env, config: StakeConfig) {
        stake::set_stake_config(&e, config);
    }

    /// Return the current staking configuration.
    pub fn get_stake_config(e: Env) -> StakeConfig {
        stake::get_stake_config(&e)
    }

    /// Return the discounted fee for `user`, taking their stake priority
    /// into account.
    ///
    /// Users with active stakes receive a percentage discount based on
    /// their priority score. Users without stakes see the raw fee.
    pub fn get_discounted_fee(e: Env, user: Address) -> i128 {
        stake::get_discounted_fee(&e, user)
    }
}

/// Token interface implementation — generated as contract functions via
/// `#[contractimpl]` so clients can call `name`, `symbol`, `decimals`,
/// and `balance_of` directly.
#[contractimpl]
impl token::TokenInterface for StellarWrapContract {
    fn name(e: Env) -> String {
        queries::name(e)
    }

    fn symbol(e: Env) -> String {
        queries::symbol(e)
    }

    fn decimals(e: Env) -> u32 {
        queries::decimals(e)
    }

    fn balance_of(e: Env, user: Address) -> i128 {
        queries::balance_of(e, user)
    }
}

#[cfg(test)]
mod admin_test;
#[cfg(test)]
mod balance_of_test;
#[cfg(test)]
mod batch_test;
#[cfg(test)]
mod bridge_test;
#[cfg(test)]
mod expiration_test;
#[cfg(test)]
mod governance_test;
#[cfg(test)]
mod last_updated_test;
#[cfg(test)]
mod oracle_test;
#[cfg(test)]
mod pause_coverage_test;
#[cfg(test)]
mod prop_test;
#[cfg(test)]
mod queries_test;
#[cfg(test)]
mod security_test;
#[cfg(test)]
mod stake_test;
#[cfg(test)]
mod invariants_test;
#[cfg(test)]
mod test;
#[cfg(test)]
mod governance_exec_test;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod test_vectors;
#[cfg(test)]
mod transfer_test;
#[cfg(test)]
mod ttl_test;
#[cfg(test)]
mod queries_test;
#[cfg(test)]
mod timelock_test;
#[cfg(test)]
mod timelock_cancel_test;
#[cfg(test)]
mod revoke_test;
