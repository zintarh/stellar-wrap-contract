// Leaf/pair helpers are kept public for parity with the off-chain tree builder
// in `scripts/merkle.ts`; not every one is called from a contract entrypoint.
#![allow(dead_code)]

use soroban_sdk::{
    panic_with_error, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

use crate::{ContractError, DataKey};

/// Domain separator for off-chain whitelist leaves.
///
/// Off-chain tree builders must prefix every leaf with these bytes so a
/// whitelist leaf can never collide with a mint payload or a claim leaf.
pub const WHITELIST_DOMAIN_SEPARATOR: &[u8; 25] = b"stellar-wrap-whitelist-v1";

/// Compute a merkle leaf: SHA-256 of Soroban XDR-encoded
/// `(user ‖ period ‖ archetype ‖ data_hash ‖ metadata)`.
pub fn compute_merkle_leaf(
    e: &Env,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    metadata: &Option<String>,
) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&user.clone().to_xdr(e));
    leaf_data.append(&period.to_xdr(e));
    leaf_data.append(&archetype.clone().to_xdr(e));
    leaf_data.append(&data_hash.clone().to_xdr(e));
    leaf_data.append(&metadata.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

/// Pair-hash for internal merkle nodes: SHA-256 of lexicographically ordered siblings.
pub fn hash_pair(e: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let a_arr = a.to_array();
    let b_arr = b.to_array();
    let mut combined = Bytes::new(e);
    if a_arr <= b_arr {
        combined.append(&Bytes::from_array(e, &a_arr));
        combined.append(&Bytes::from_array(e, &b_arr));
    } else {
        combined.append(&Bytes::from_array(e, &b_arr));
        combined.append(&Bytes::from_array(e, &a_arr));
    }
    let hash = e.crypto().sha256(&combined);
    BytesN::from_array(e, &hash.to_array())
}

/// Verify a merkle proof against `root`. `proof` is ordered from leaf sibling to root.
pub fn verify_merkle_proof(
    e: &Env,
    root: &BytesN<32>,
    leaf: &BytesN<32>,
    proof: &Vec<BytesN<32>>,
) -> bool {
    let mut computed = leaf.clone();
    for sibling in proof.iter() {
        computed = hash_pair(e, &computed, &sibling);
    }
    computed == *root
}

/// Compute the whitelist leaf for `user`: SHA-256 of
/// `(WHITELIST_DOMAIN_SEPARATOR ‖ XDR(user))`.
pub fn compute_whitelist_leaf(e: &Env, user: &Address) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&Bytes::from_array(e, WHITELIST_DOMAIN_SEPARATOR));
    leaf_data.append(&user.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

/// Admin-only: publish the merkle root committing to the off-chain whitelist.
///
/// The whitelist itself never touches the chain — only its 32-byte root. A new
/// root fully replaces the previous one, so rotating the whitelist is a single
/// cheap write. Emits a `("whitelist", "root")` event for indexers.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn set_whitelist_root(e: Env, root: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    crate::admin::read_admin(&e).require_auth();
    e.storage().instance().set(&DataKey::WhitelistRoot, &root);
    e.events()
        .publish((symbol_short!("whitelist"), symbol_short!("root")), root);
}

/// Admin-only: remove the whitelist root, disabling whitelist gating.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn clear_whitelist_root(e: Env) {
    crate::timelock::require_direct_call_allowed(&e);
    crate::admin::read_admin(&e).require_auth();
    e.storage().instance().remove(&DataKey::WhitelistRoot);
    e.events()
        .publish((symbol_short!("whitelist"), symbol_short!("cleared")), ());
}

/// Return the currently published whitelist root, if any.
pub(crate) fn get_whitelist_root(e: &Env) -> Option<BytesN<32>> {
    e.storage().instance().get(&DataKey::WhitelistRoot)
}

/// Read the whitelist root or panic with `MerkleRootNotSet`.
fn read_whitelist_root(e: &Env) -> BytesN<32> {
    get_whitelist_root(e).unwrap_or_else(|| panic_with_error!(e, ContractError::MerkleRootNotSet))
}

/// Check whether `user` is a member of the published whitelist.
///
/// Returns `false` for a malformed or non-matching proof instead of panicking,
/// so callers can use this as a cheap read-only membership query.
///
/// # Panics
/// - [`ContractError::MerkleRootNotSet`] if no root has been published.
pub(crate) fn verify_whitelist(e: Env, user: Address, proof: Vec<BytesN<32>>) -> bool {
    let root = read_whitelist_root(&e);
    let leaf = compute_whitelist_leaf(&e, &user);
    verify_merkle_proof(&e, &root, &leaf, &proof)
}

/// Panicking variant of [`verify_whitelist`] for use as a gate inside other
/// contract functions.
///
/// # Panics
/// - [`ContractError::MerkleRootNotSet`] if no root has been published.
/// - [`ContractError::InvalidMerkleProof`] if `proof` does not prove membership.
pub(crate) fn require_whitelisted(e: &Env, user: &Address, proof: &Vec<BytesN<32>>) {
    let root = read_whitelist_root(e);
    let leaf = compute_whitelist_leaf(e, user);
    if !verify_merkle_proof(e, &root, &leaf, proof) {
        panic_with_error!(e, ContractError::InvalidMerkleProof);
    }
}
