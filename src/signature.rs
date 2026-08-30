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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MintPayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub payload_version: u32,
    pub valid_until: u64,
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
    valid_until: u64,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, MINT_DOMAIN_SEPARATOR));

    let typed_payload = MintPayload {
        archetype: archetype.clone(),
        contract_id: contract_id.clone(),
        data_hash: data_hash.clone(),
        payload_version,
        valid_until,
        period,
        user: user.clone(),
    };

    payload.append(&typed_payload.to_xdr(e));
    payload
}

/// Verifies an Ed25519 signature in-guest, mapping every failure mode to
/// [`ContractError::InvalidSignature`].
///
/// The host `ed25519_verify` primitive cannot produce the contract error: on a
/// bad signature it traps the VM with an uncatchable `Error(Crypto,
/// InvalidInput)` host error (soroban-sdk `Crypto::ed25519_verify` discards the
/// result, so the guest never regains control). Verifying here with the same
/// pinned `ed25519-dalek` version (3.0.0, `verify_strict`) that
/// `soroban-env-host` uses reproduces identical acceptance semantics while
/// keeping the failure inside the contract's error domain.
fn verify_ed25519(
    public_key: &BytesN<32>,
    message: &Bytes,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let verifying_key = VerifyingKey::from_bytes(&public_key.to_array())
        .map_err(|_| ContractError::InvalidSignature)?;
    let sig = Signature::from_bytes(&signature.to_array());

    let mut msg = [0u8; 512];
    let len = message.len() as usize;
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
/// Every rejection — malformed key, tampered payload, wrong key, corrupted
/// signature — surfaces as [`ContractError::InvalidSignature`].
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
    valid_until: u64,
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let payload = construct_mint_payload(
        e,
        contract_id,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
        valid_until,
    );
    verify_ed25519(admin_pubkey, &payload, signature)
}

/// Domain separator used for batch aggregated signatures.
pub const BATCH_MINT_DOMAIN_SEPARATOR: &[u8; 21] = b"stellar-wrap-batch-v1";

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
/// Any rejection surfaces as [`ContractError::InvalidSignature`].
pub fn verify_batch_aggregated_signature(
    e: &Env,
    admin_pubkey: &BytesN<32>,
    contract_id: &Address,
    items: &soroban_sdk::Vec<crate::storage_types::BatchWrapItem>,
    payload_version: u32,
    aggregated_signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let payload = construct_batch_mint_payload(e, contract_id, items, payload_version);
    verify_ed25519(admin_pubkey, &payload, aggregated_signature)
}

pub const INBOUND_BRIDGE_DOMAIN_SEPARATOR: &[u8; 18] = b"stellar-bridge-in1";

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InboundBridgePayload {
    pub archetype: Symbol,
    pub contract_id: Address,
    pub data_hash: BytesN<32>,
    pub period: u64,
    pub recipient: Address,
    pub source_chain: u32,
    pub source_nonce: u64,
}

pub fn construct_inbound_bridge_payload(
    e: &Env,
    contract_id: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> Bytes {
    let mut payload = Bytes::new(e);
    payload.append(&Bytes::from_array(e, INBOUND_BRIDGE_DOMAIN_SEPARATOR));

    let typed_payload = InboundBridgePayload {
        archetype: archetype.clone(),
        contract_id: contract_id.clone(),
        data_hash: data_hash.clone(),
        period,
        recipient: recipient.clone(),
        source_chain,
        source_nonce,
    };

    payload.append(&typed_payload.to_xdr(e));
    payload
}

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
    signature: &BytesN<64>,
) -> Result<(), ContractError> {
    let payload = construct_inbound_bridge_payload(
        e,
        contract_id,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
    );
    verify_ed25519(relayer_pubkey, &payload, signature)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
mod tests {
    extern crate std;

    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use crate::StellarWrapContract;
    use crate::StellarWrapContractClient;

    fn sign_payload(
        env: &Env,
        signer: &SigningKey,
        contract: &Address,
        user: &Address,
        period: u64,
        archetype: &Symbol,
        data_hash: &BytesN<32>,
        payload_version: u32,
    ) -> BytesN<64> {
        let payload = construct_mint_payload(
            env,
            contract,
            user,
            period,
            archetype,
            data_hash,
            payload_version,
        );

        let mut out = [0u8; 512];
        let len = payload.len() as usize;
        payload.copy_into_slice(&mut out[..len]);

        let signature = signer.sign(&out[..len]);
        BytesN::from_array(env, &signature.to_bytes())
    }

    #[test]
    fn test_construct_mint_payload_has_expected_byte_layout() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[42u8; 32]);
        let period = 202512u64;

        let payload =
            construct_mint_payload(&env, &contract_id, &user, period, &archetype, &data_hash, 1);

        let mut expected = Bytes::new(&env);
        expected.append(&Bytes::from_array(&env, MINT_DOMAIN_SEPARATOR));

        let typed_payload = MintPayload {
            archetype: archetype.clone(),
            contract_id: contract_id.clone(),
            data_hash: data_hash.clone(),
            payload_version: 1,
            period,
            user: user.clone(),
        };
        expected.append(&typed_payload.to_xdr(&env));

        assert_eq!(payload, expected);
    }

    #[test]
    fn test_verify_mint_signature_accepts_valid_signature() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[7u8; 32]);
        let period = 202601u64;

        let signing_key = SigningKey::from_bytes(&[11u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
            1,
        );

        assert!(verify_mint_signature(
            &env,
            &admin_pubkey,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
            1,
            &signature,
        )
        .is_ok());
    }

    #[test]
    fn test_verify_mint_signature_rejects_invalid_signature() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[8u8; 32]);
        let period = 202602u64;

        let signing_key = SigningKey::from_bytes(&[12u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let invalid_signature = BytesN::from_array(&env, &[0u8; 64]);

        let result = catch_unwind(AssertUnwindSafe(|| {
            verify_mint_signature(
                &env,
                &admin_pubkey,
                &contract_id,
                &user,
                period,
                &archetype,
                &data_hash,
                1,
                &invalid_signature,
            )
            .unwrap();
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_verify_mint_signature_rejects_wrong_key() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let user = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[9u8; 32]);
        let period = 202603u64;

        let signing_key = SigningKey::from_bytes(&[13u8; 32]);
        let wrong_signing_key = SigningKey::from_bytes(&[14u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let wrong_signature = sign_payload(
            &env,
            &wrong_signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
            1,
        );

        let result = catch_unwind(AssertUnwindSafe(|| {
            verify_mint_signature(
                &env,
                &admin_pubkey,
                &contract_id,
                &user,
                period,
                &archetype,
                &data_hash,
                1,
                &wrong_signature,
            )
            .unwrap();
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_mint_wrap_rejects_invalid_signature_length() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let client = StellarWrapContractClient::new(&env, &contract_id);

        let signing_key = SigningKey::from_bytes(&[99u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        client.initialize(&admin, &admin_pubkey);
        env.mock_all_auths();

        let data_hash = BytesN::from_array(&env, &[42u8; 32]);
        let archetype = symbol_short!("arch");
        let period = 202512u64;
        let invalid_sig = BytesN::from_array(&env, &[0u8; 64]);

        let result = catch_unwind(AssertUnwindSafe(|| {
            client.mint_wrap(&user, &period, &archetype, &data_hash, &1u32, &invalid_sig);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn test_verify_batch_aggregated_signature_success() {
        let env = Env::default();
        let contract_id = env.register(StellarWrapContract, ());
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&env, &[1u8; 32]);
        let period1 = 202601u64;
        let period2 = 202602u64;

        let item1 = crate::storage_types::BatchWrapItem {
            user: user1,
            period: period1,
            archetype: archetype.clone(),
            data_hash: data_hash.clone(),
            payload_version: 1,
            signature: BytesN::from_array(&env, &[0u8; 64]),
        };
        let item2 = crate::storage_types::BatchWrapItem {
            user: user2,
            period: period2,
            archetype: archetype.clone(),
            data_hash: data_hash.clone(),
            payload_version: 1,
            signature: BytesN::from_array(&env, &[0u8; 64]),
        };

        let mut items = soroban_sdk::Vec::new(&env);
        items.push_back(item1);
        items.push_back(item2);

        let signing_key = SigningKey::from_bytes(&[25u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

        let payload = construct_batch_mint_payload(&env, &contract_id, &items, 1);
        let mut out = [0u8; 512];
        let len = payload.len() as usize;
        payload.copy_into_slice(&mut out[..len]);

        let agg_sig_bytes = signing_key.sign(&out[..len]);
        let agg_sig = BytesN::from_array(&env, &agg_sig_bytes.to_bytes());

        assert!(verify_batch_aggregated_signature(
            &env,
            &admin_pubkey,
            &contract_id,
            &items,
            1,
            &agg_sig,
        )
        .is_ok());
    }
}
