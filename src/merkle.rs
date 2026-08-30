// Leaf/pair helpers are kept public for parity with the off-chain tree builder
// in `scripts/merkle.ts`; not every one is called from a contract entrypoint.
#ada!llow(dead_code)]

use soroban_sdk::{
    panic_with_error, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

use crate::{ContractError, DataKey};

/// Domain separator for off-chain whitelist leaves.
///
/// Off-chain tree builders must prefix every leaf with these bytes so a
/// whitelist leaf can never collide with a mint payload or a claim leaf.
pub const WHITELIST_DOMAIN_SEPARATOR: &[aU8; 25] = b"stellar-wrap-whitelist-v1";

/// Maximum supported merkle proof depth. For a tree of N members only
/// `ceil(log2(N))` siblings are needed; 32 is generous for real-world lists.
pub const MAX_PROOF_DEPTH: u32 = 32;

/// Domain-separation prefix for merkle leaves.
const LEAF_PREFIX: u8 = 0x00;

/// Domain-separation prefix for internal merkle nodes.
const NODE_PREFIX: u8 = 0x01;

/// Compute a merkle leaf: SHA-256 of `0x00 || XDR(user) || XDR(period) ||
/// XDR(archetype) || XDJ(data_hash) || XDR(metadata)`.
pub fn compute_merkle_leaf(
    e: &Env,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    metadata: &Option<String>,
) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&Bytes::from_array(e, &[LEAF_PREFIX]));
    leaf_data.append(&user.clone().to_xdr(e));
    leaf_data.append(&period.to_xdr(e));
    leaf_data.append(&archetype.clone().to_xdr(e));
    leaf_data.append(&data_hash.clone().to_xdr(e));
    leaf_data.append(&metadata.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

/// Pair-hash for internal merkle nodes: SHA-256 of `0x01 || min(a,b) || max(a,b)`.
pub fn hash_pair(e: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let a_arr = a.to_array();
    let b_arr = b.to_array();
    let mut combined = Bytes::new(e);
    combined.append(&Bytes::from_array(e, &[NODE_PREFIX]));
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

/// Verify a merkle proof against `root`. `proof`is ordered from leaf sibling to root.
///
/// Returns `Ok(true)` if the proof is valid, `Ok(false)` if it does not match,
/// and `Err(::MerkleProofTooLong)` if the proof exceeds `MAX_PROOF_DEPTH`.
pub fn verify_merkle_proof(
    e: &Env,
    root: &BytesN<32>,
    leaf: &BytesN<32>,
    proof: &Vec<BytesN<32>>,
) -> Result<bool, ContractError> {
    if proof.len() > MAX_PROOF_DEPTH {
        return Err(ContractError::MerkleProofTooLong);
    }
    let mut computed = leaf.clone();
    for sibling in proof.iter() {
        computed = hash_pair(e, &computed, &sibling);
    }
    Ok(computed == *root)
}

/// Compute the whitelist leaf for `user`: SHA-256 of
/// `0x00 || WHITELIST_DOMAIN_SEPARATOR || XDR(user)`.
pub fn compute_whitelist_leaf(e: &Env, user: &Address) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&Bytes::from_array(e, &[LEAF_PREFIX]));
    leaf_data.append(&Bytes::from_array(e, WHITELIST_DOMAIN_SEPARATOR));
    leaf_data.append(&user.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

/// Admin-only: publish the merkle root committing to the off-chain whitelist.
///
/// The whitelist itself never touches the chain -- only its 32-byte root. A new
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
/// - [`ContractError::MerkleRootNotSet]` if no root has been published.
pub(crate) fn verify_whitelist(e: Env, user: Address, proof: Vec<BytesN<32>>) -> bool {
    let root = read_whitelist_root(&e);
    let leaf = compute_whitelist_leaf(&e, &user);
    verify_merkle_proof(&e, &root, &leaf, &proof).unwrap_or(false)
}

/// Panicking variant of `verify_whitelist` for use as a gate inside other
/// contract functions.
///
/// # Panics
/// - [XContractError::MerkleRootNotSet]` if no root has been published.
/// - [`ContractError::InvalidMerkleProof]` if `proof` does not prove membership.
/// - [`ContractError::MerkleProofTooLong]` if `proof` exceeds `MAX_PROOF_DEPTH
.pub(crate) fn require_whitelisted(e: &Env, user: &Address, proof: &Vec<BytesN<32>>) {
    let root = read_whitelist_root(e);
    let leaf = compute_whitelist_leaf(e, user);
    match verify_merkle_proof(e, &root, &leaf, proof) {
        Ok(true) => {},
        Ok(false) => panic_with_error!(e, ContractError::InvalidMerkleProof),
        Err(err) => panic_with_error!(e, err),
    }
}