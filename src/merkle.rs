// Pure Merkle-tree crypto helpers.
//
// These utilities are kept in the registry for parity with the off-chain tree
// builder in `scripts/merkle.ts` and for use by satellite contracts.  All
// whitelist-specific entrypoints and storage were removed as part of the v2
// architecture decision: the backend Ed25519 signature is the sole mint gate,
// making an on-chain Merkle whitelist redundant.  See
// `docs/whitelist-merkle.md` for the decision record.

use soroban_sdk::{panic_with_error, Bytes, BytesN, Env, String, Symbol, Vec};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::Address;

use crate::constants::{MAX_PROOF_DEPTH, MERKLE_LEAF_PREFIX, MERKLE_NODE_PREFIX};
use crate::ContractError;

/// Compute the Merkle leaf for a wrap record.
///
/// Leaf = SHA-256( 0x00 || XDR(user) || XDR(period) || XDR(archetype)
///                      || XDR(data_hash) || XDR(metadata) )
///
/// The 0x00 prefix (`MERKLE_LEAF_PREFIX`) domain-separates leaves from
/// internal nodes (prefixed `MERKLE_NODE_PREFIX` = 0x01).
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

/// Combine two sibling hashes into their parent node.
///
/// Node = SHA-256( 0x01 || min(a, b) || max(a, b) )
///
/// Sorting siblings lexicographically means proofs do not need explicit
/// left/right position flags.
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

/// Verify a Merkle inclusion proof.
///
/// Returns `true` when folding `proof` from `leaf` up to the root produces
/// exactly `root`.  Panics with `MerkleProofTooLong` when the proof depth
/// exceeds `MAX_PROOF_DEPTH`.
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
