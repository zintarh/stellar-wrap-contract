extern crate alloc;

use ed25519_dalek::{Signature, VerifyingKey};
use soroban_sdk::{contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

use crate::ContractError;

/// Domain separator used for mint signatures.
///
/// Off-chain signers must construct the same byte sequence before signing.
/// Including a domain separator makes the payload self-describing and prevents
/// ambiguity if the same key is reused for other Soroban contracts or future
/// signing schemes.
pub const MINT_DOMAIN_SEPARATOR: &[u8; 15] = b"stellar-wrap-v1";

/// Current mint signature scheme version.
///
/// This is a scheme discriminant, distinct from the period and data hash. It
/// identifies the construction used to build the signed payload so that a
/// future scheme can be introduced without invalidating every outstanding
/// signature at once. See [`SUPPORTED_MINT_SCHEME_VERSIONS`] for the migration
/// window semantics.
pub const MINT_SCHEME_VERSION: u32 = 1;

/// Scheme versions accepted by [`verify_mint_signature`].
///
/// During a migration window more than one version may be listed so that
/// signatures produced under the previous scheme remain valid while signers
/// roll over to the new one. Retiring a scheme is done by removing its version
/// from this set once the migration window closes; payloads carrying a version
/// that is not present here are rejected with
/// [`ContractError::InvalidSignature`] rather than being assumed to use the
/// current scheme.
pub const SUPPORTED_MINT_SCHEME_VERSIONS: &[u32] = &[MINT_SCHEME_VERSION];

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintPayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub period: u64,
    pub user: Address,
}

/// Construct the canonical mint payload that is signed by the admin.
///
/// The payload is the concatenation of a domain separator, the contract ID,
/// the user address, the period, the archetype, and the data hash. Each field
/// is encoded with XDR so the byte layout is deterministic and unambiguous.
pub fn construct_mint_payload(
    e: &Env,
    contract_id: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, MINT_DOMAIN_SEPARATOR));

    let typed_payload = MintPayload {
        archetype: archetype.clone(),
        contract_id: contract_id.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        period,
        user: user.clone(),
    };

    payload.append(&typed_payload.to_xdr(e));
    payload
}

/// Maximum payload size accepted by `verify_ed25519`.
///
/// A batch of `MAX_BATCH_SIZE` items produces a payload well under this limit
/// (~16 KiB headroom). Payloads that exceed this bound are rejected with
/// [`ContractError::InvalidSignature`] rather than panicking.
pub const MAX_VERIFY_PAYLOAD_BYTES: usize = 32_768; // 32 KiB

/// Verifies an Ed25519 signature in-guest, mapping every failure mode to
/// [`ContractError::InvalidSignature`].
///
/// The host `ed25519_verify` primitive cannot produce the contract error: on a
/// bad signature it traps the VM with an uncatchable `Error(Crypto,
/// InvalidInput)` host error (soroban-sdk `Crypto::ed25519_verify` discards the
/// result, so the guest never regains control). Verifying here reproduces
/// acceptance semantics inside the contract's error domain at the cost of
/// ~45.5 KB bytecode overhead. See `SIGNATURE_VERIFICATION_DECISION.md` for full
/// measurement and architectural trade-off details.
///
/// # Panics
///
/// Never panics. Payloads larger than [`MAX_VERIFY_PAYLOAD_BYTES`] are
/// rejected with [`ContractError::InvalidSignature`].
fn verify_ed25519(
    public_key: &BytesN<32>,
    message: &Bytes,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let len = message.len() as usize;
    if len > MAX_VERIFY_PAYLOAD_BYTES {
        return Err(ContractError::InvalidSignature);
    }

    let verifying_key = VerifyingKey::from_bytes(&public_key.to_array())
        .map_err(|_| ContractError::InvalidSignature)?;
    let sig = Signature::from_bytes(&signature.to_array());

    let mut msg = [0u8; MAX_VERIFY_PAYLOAD_BYTES];
    message.copy_into_slice(&mut msg[..len]);

    verifying_key
        .verify_strict(&msg[..len], &sig)
        .map_err(|_| ContractError::InvalidSignature)
}

/// Verify an admin signature for a wrap mint request.
///
/// The verification is performed over the canonical mint payload so the
/// signature is bound to the current contract instance, the target user,
/// the period, the archetype, and the data hash.
///
/// The `payload_version` argument is the scheme discriminant. It is rejected
/// with [`ContractError::InvalidSignature`] unless it appears in
/// [`SUPPORTED_MINT_SCHEME_VERSIONS`], so an unrecognized scheme is never
/// silently treated as the current one.
///
/// Every rejection — malformed key, tampered payload, wrong key, corrupted
/// signature, unsupported scheme version — surfaces as
/// [`ContractError::InvalidSignature`].
#[allow(clippy::too_many_arguments)]
pub fn verify_mint_signature(
    e: &Env,
    admin_pubkey: &BytesN<32>,
    contract_id: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    if !SUPPORTED_MINT_SCHEME_VERSIONS.contains(&payload_version) {
        return Err(ContractError::InvalidSignature);
    }

    let payload = construct_mint_payload(
        e,
        contract_id,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
    );
    verify_ed25519(admin_pubkey, &payload, signature)
}

/// Domain separator used for batch aggregated signatures.
pub const BATCH_MINT_DOMAIN_SEPARATOR: &[u8; 21] = b"stellar-wrap-batch-v1";

/// Current batch mint signature scheme version.
pub const BATCH_MINT_SCHEME_VERSION: u32 = 1;

/// Scheme versions accepted by [`verify_batch_aggregated_signature`].
///
/// Same migration semantics as [`SUPPORTED_MINT_SCHEME_VERSIONS`]: multiple
/// versions may coexist during a migration window, and a version is retired by
/// removing it from this set.
pub const SUPPORTED_BATCH_MINT_SCHEME_VERSIONS: &[u32] = &[BATCH_MINT_SCHEME_VERSION];

/// Construct the canonical batch payload representing a commitment to an ordered set of batch wrap items.
pub fn construct_batch_mint_payload(
    e: &Env,
    contract_id: &Address,
    items: &soroban_sdk::Vec<crate::storage_types::BatchWrapItem>,
    payload_version: u32,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, BATCH_MINT_DOMAIN_SEPARATOR));
    payload.append(&payload_version.to_xdr(e));
    payload.append(&contract_id.to_xdr(e));
    payload.append(&items.len().to_xdr(e));
    for item in items.iter() {
        payload.append(&item.user.to_xdr(e));
        payload.append(&item.period.to_xdr(e));
        payload.append(&item.archetype.to_xdr(e));
        payload.append(&item.data_hash.to_xdr(e));
    }
    payload
}

/// Verify an aggregated batch signature over a set of batch wrap items.
///
/// The `payload_version` argument is the scheme discriminant and is rejected
/// with [`ContractError::InvalidSignature`] unless it appears in
/// [`SUPPORTED_BATCH_MINT_SCHEME_VERSIONS`].
///
/// Any rejection surfaces as [`ContractError::InvalidSignature`].
pub fn verify_batch_aggregated_signature(
    e: &Env,
    admin_pubkey: &BytesN<32>,
    contract_id: &Address,
    items: &soroban_sdk::Vec<crate::storage_types::BatchWrapItem>,
    payload_version: u32,
    aggregated_signature: &BytesN<64>,
) -> Result<(), ContractError> {
    if !SUPPORTED_BATCH_MINT_SCHEME_VERSIONS.contains(&payload_version) {
        return Err(ContractError::InvalidSignature);
    }

    let payload = construct_batch_mint_payload(e, contract_id, items, payload_version);
    verify_ed25519(admin_pubkey, &payload, aggregated_signature)
}

pub const INBOUND_BRIDGE_DOMAIN_SEPARATOR: &[u8; 18] = b"stellar-bridge-in1";

/// Current inbound bridge signature scheme version.
pub const INBOUND_BRIDGE_SCHEME_VERSION: u32 = 1;

/// Scheme versions accepted by [`verify_inbound_bridge_signature`].
///
/// Same migration semantics as [`SUPPORTED_MINT_SCHEME_VERSIONS`].
pub const SUPPORTED_INBOUND_BRIDGE_SCHEME_VERSIONS: &[u32] = &[INBOUND_BRIDGE_SCHEME_VERSION];

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundBridgePayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub period: u64,
    pub recipient: Address,
    pub source_chain: u32,
    pub source_nonce: u64,
}

#[allow(clippy::too_many_arguments)]
pub fn construct_inbound_bridge_payload(
    e: &Env,
    contract_id: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, INBOUND_BRIDGE_DOMAIN_SEPARATOR));

    let typed_payload = InboundBridgePayload {
        archetype: archetype.clone(),
        contract_id: contract_id.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        period,
        recipient: recipient.clone(),
        source_chain,
        source_nonce,
    };

    payload.append(&typed_payload.to_xdr(e));
    payload
}

#[allow(clippy::too_many_arguments)]
pub fn verify_inbound_bridge_signature(
    e: &Env,
    relayer_pubkey: &BytesN<32>,
    contract_id: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    if !SUPPORTED_INBOUND_BRIDGE_SCHEME_VERSIONS.contains(&payload_version) {
        return Err(ContractError::InvalidSignature);
    }

    let payload = construct_inbound_bridge_payload(
        e,
        contract_id,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
        payload_version,
    );
    verify_ed25519(relayer_pubkey, &payload, signature)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
mod tests {
    extern crate std;
    use std::vec;

    use std::panic::{catch_unwind, AssertUnwindSafe};

    use ed25519_dalek::{Signer, SigningKey};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};

    use super::*;
    use crate::

/* … truncated 11755 chars — edit only what you need near the top … */
