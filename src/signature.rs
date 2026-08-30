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

/// Maximum allowed payload size for Ed25519 signature verification.
/// This limit ensures that payloads can be processed without unreasonable
/// memory usage while supporting the maximum expected batch size.
/// 16KB allows for batches well beyond MAX_BATCH_SIZE with room to grow.
pub const MAX_SIGNATURE_PAYLOAD_SIZE: usize = 16384; // 16KB

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

    let len = message.len() as usize;
    
    // Reject oversized payloads to prevent excessive memory usage and ensure
    // reasonable resource consumption. This returns a proper ContractError
    // instead of causing a VM trap from buffer overrun.
    if len > MAX_SIGNATURE_PAYLOAD_SIZE {
        return Err(ContractError::InvalidSignature);
    }

    // Allocate a buffer sized to the actual message length to avoid
    // fixed-size buffer limitations
    extern crate alloc;
    use alloc::vec::Vec;
    
    let mut msg_bytes = Vec::with_capacity(len);
    msg_bytes.resize(len, 0u8);
    message.copy_into_slice(&mut msg_bytes);

    verifying_key
        .verify_strict(&msg_bytes, &sig)
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

        extern crate alloc;
        use alloc::vec::Vec;
        
        let len = payload.len() as usize;
        let mut out = Vec::with_capacity(len);
        out.resize(len, 0u8);
        payload.copy_into_slice(&mut out);

        let signature = signer.sign(&out);
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
        
        extern crate alloc;
        use alloc::vec::Vec;
        
        let len = payload.len() as usize;
        let mut out = Vec::with_capacity(len);
        out.resize(len, 0u8);
        payload.copy_into_slice(&mut out);

        let agg_sig_bytes = signing_key.sign(&out);
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

    #[test]
    fn test_verify_ed25519_rejects_oversized_payload() {
        let env = Env::default();
        let signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        
        // Create a payload larger than MAX_SIGNATURE_PAYLOAD_SIZE (16384 bytes)
        let oversized_data = [0u8; super::MAX_SIGNATURE_PAYLOAD_SIZE + 1];
        let oversized_payload = Bytes::from_array(&env, &oversized_data);
        let signature = BytesN::from_array(&env, &[0u8; 64]);
        
        let result = super::verify_ed25519(&admin_pubkey, &oversized_payload, &signature);
        assert_eq!(result, Err(ContractError::InvalidSignature));
    }

    #[test]
    fn test_batch_mint_with_max_size() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarWrapContract);
        let client = StellarWrapContractClient::new(&env, &contract_id);

        let signing_key = SigningKey::from_bytes(&[50u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let admin = Address::generate(&env);

        client.initialize(&admin, &admin_pubkey);
        env.mock_all_auths();

        // Create a batch with MAX_BATCH_SIZE (100) items
        let mut items = soroban_sdk::Vec::new(&env);
        for i in 0..crate::mint::MAX_BATCH_SIZE {
            let user = Address::generate(&env);
            let item = crate::storage_types::BatchWrapItem {
                user,
                period: 202601u64 + (i as u64),
                archetype: symbol_short!("test"),
                data_hash: BytesN::from_array(&env, &[i as u8; 32]),
                payload_version: 1,
                signature: BytesN::from_array(&env, &[0u8; 64]),
            };
            items.push_back(item);
        }

        // Create aggregated signature
        let payload = construct_batch_mint_payload(&env, &contract_id, &items, 1);
        
        extern crate alloc;
        use alloc::vec::Vec;
        
        let len = payload.len() as usize;
        let mut out = Vec::with_capacity(len);
        out.resize(len, 0u8);
        payload.copy_into_slice(&mut out);

        let agg_sig_bytes = signing_key.sign(&out);
        let agg_sig = BytesN::from_array(&env, &agg_sig_bytes.to_bytes());

        // This should succeed without panicking
        client.mint_wrap_batch(&items, &Some(agg_sig));
        
        // Verify all items were minted
        for item in items.iter() {
            assert!(client.has_wrap(&item.user, &item.period));
        }
    }

    #[test]
    fn test_single_mint_with_maximum_archetype_symbol() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarWrapContract);
        let client = StellarWrapContractClient::new(&env, &contract_id);

        let signing_key = SigningKey::from_bytes(&[75u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        client.initialize(&admin, &admin_pubkey);
        env.mock_all_auths();

        // Create a maximum-length archetype symbol (32 characters)
        let max_archetype = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz123456");
        let data_hash = BytesN::from_array(&env, &[42u8; 32]);
        let period = 202601u64;

        let signature = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &max_archetype,
            &data_hash,
            1,
        );

        // This should succeed without panicking
        client.mint_wrap(&user, &period, &max_archetype, &data_hash, &1u32, &signature);
        
        // Verify the wrap was created
        assert!(client.has_wrap(&user, &period));
    }

    #[test]
    fn test_batch_payload_size_within_limits() {
        let env = Env::default();
        let contract_id = env.register_contract(None, StellarWrapContract);

        // Create a batch with MAX_BATCH_SIZE items to verify payload size
        let mut items = soroban_sdk::Vec::new(&env);
        for i in 0..crate::mint::MAX_BATCH_SIZE {
            let user = Address::generate(&env);
            // Use maximum-length archetype to test worst case
            let max_archetype = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyz123456");
            let item = crate::storage_types::BatchWrapItem {
                user,
                period: 202601u64 + (i as u64),
                archetype: max_archetype,
                data_hash: BytesN::from_array(&env, &[(i % 256) as u8; 32]),
                payload_version: 1,
                signature: BytesN::from_array(&env, &[0u8; 64]),
            };
            items.push_back(item);
        }

        let payload = construct_batch_mint_payload(&env, &contract_id, &items, 1);
        let payload_size = payload.len() as usize;
        
        // Verify the payload size is within our declared limits
        assert!(payload_size <= crate::mint::MAX_BATCH_PAYLOAD_SIZE);
        assert!(payload_size <= MAX_SIGNATURE_PAYLOAD_SIZE);
    }
}
