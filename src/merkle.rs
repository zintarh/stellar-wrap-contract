// Leaf/pair helpers are kept public for parity with the off-chain tree builder
// in `scripts/merkle.ts`; not every one is called from a contract entrypoint.

use soroban_sdk::{
    panic_with_error, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

use crate::constants::{MAX_PROOF_DEPTH, MERKLE_LEAF_PREFIX, MERKLE_NODE_PREFIX};
use crate::{ContractError, DataKey};

pub const WHITELIST_DOMAIN_SEPARATOR: &[u8; 25] = b"stellar-wrap-whitelist-v1";

pub fn compute_merkle_leaf(
    e: &Env,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    metadata: &Option<String>,
) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&Bytes::from_array(e, &[MERKLE_LEAF_PREFIX]));
    leaf_data.append(&user.clone().to_xdr(e));
    leaf_data.append(&period.to_xdr(e));
    leaf_data.append(&archetype.clone().to_xdr(e));
    leaf_data.append(&data_hash.clone().to_xdr(e));
    leaf_data.append(&metadata.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

pub fn hash_pair(e: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let a_array = a.to_array();
    let b_array = b.to_array();
    let mut combined = Bytes::new(e);
    combined.append(&Bytes::from_array(e, &[MERKLE_NODE_PREFIX]));
    if a_array <= b_array {
        combined.append(&Bytes::from_array(e, &a_array));
        combined.append(&Bytes::from_array(e, &b_array));
    } else {
        combined.append(&Bytes::from_array(e, &b_array));
        combined.append(&Bytes::from_array(e, &a_array));
    }
    let hash = e.crypto().sha256(&combined);
    BytesN::from_array(e, &hash.to_array())
}

pub fn verify_merkle_proof(
    e: &Env,
    root: &BytesN<32>,
    leaf: &BytesN<32>,
    proof: &Vec<BytesN<32>>,
) -> bool {
    if proof.len() > MAX_PROOF_DEPTH {
        panic_with_error!(e, ContractError::MerkleProofTooLong);
    }
    let mut computed = leaf.clone();
    for sibling in proof.iter() {
        computed = hash_pair(e, &computed, &sibling);
    }
    computed == *root
}

pub fn compute_whitelist_leaf(e: &Env, user: &Address) -> BytesN<32> {
    let mut leaf_data = Bytes::new(e);
    leaf_data.append(&Bytes::from_array(e, &[MERKLE_LEAF_PREFIX]));
    leaf_data.append(&Bytes::from_array(e, WHITELIST_DOMAIN_SEPARATOR));
    leaf_data.append(&user.clone().to_xdr(e));
    let hash = e.crypto().sha256(&leaf_data);
    BytesN::from_array(e, &hash.to_array())
}

#[allow(deprecated)]
pub(crate) fn set_whitelist_root(e: Env, root: BytesN<32>) {
    crate::timelock::require_direct_call_allowed(&e);
    crate::admin::read_admin(&e).require_auth();
    e.storage().instance().set(&DataKey::WhitelistRoot, &root);
    e.events()
        .publish((symbol_short!("whitelist"), symbol_short!("root")), root);
}

#[allow(deprecated)]
pub(crate) fn clear_whitelist_root(e: Env) {
    crate::timelock::require_direct_call_allowed(&e);
    crate::admin::read_admin(&e).require_auth();
    e.storage().instance().remove(&DataKey::WhitelistRoot);
    e.events()
        .publish((symbol_short!("whitelist"), symbol_short!("cleared")), ());
}

pub(crate) fn get_whitelist_root(e: &Env) -> Option<BytesN<32>> {
    e.storage().instance().get(&DataKey::WhitelistRoot)
}

fn read_whitelist_root(e: &Env) -> BytesN<32> {
    get_whitelist_root(e).unwrap_or_else(|| panic_with_error!(e, ContractError::MerkleRootNotSet))
}

pub(crate) fn verify_whitelist(e: Env, user: Address, proof: Vec<BytesN<32>>) -> bool {
    let root = read_whitelist_root(&e);
    let leaf = compute_whitelist_leaf(&e, &user);
    verify_merkle_proof(&e, &root, &leaf, &proof)
}

pub(crate) fn require_whitelisted(e: &Env, user: &Address, proof: &Vec<BytesN<32>>) {
    let root = read_whitelist_root(e);
    let leaf = compute_whitelist_leaf(e, user);
    if !verify_merkle_proof(e, &root, &leaf, proof) {
        panic_with_error!(e, ContractError::InvalidMerkleProof);
    }
}
