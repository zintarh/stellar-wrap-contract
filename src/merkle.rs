use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol, Vec};

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
