#![cfg(test)]

use crate::mint::CURRENT_PAYLOAD_VERSION;
use crate::test_utils::sign_payload;
use crate::{StellarWrapContract, StellarWrapContractClient};
use ed25519_dalek::SigningKey;
use soroban_sdk::{symbol_short, testutils::Address as _, Address, BytesN, Env};

#[test]
fn test_total_revoked_counter() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);

    // total_revoked() is 0 on a fresh contract.
    assert_eq!(client.total_revoked(), 0);

    client.initialize(&admin, &admin_pubkey);
    env.mock_all_auths();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let reason_hash = BytesN::from_array(&env, &[0u8; 32]);

    // Setup 2 wraps for user1, 1 wrap for user2
    for period in [202401u64, 202402u64] {
        let hash = BytesN::from_array(&env, &[period as u8; 32]);
        let sig = sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user1,
            period,
            &archetype,
            &hash,
        );
        client.mint_wrap(
            &user1,
            &period,
            &archetype,
            &hash,
            &CURRENT_PAYLOAD_VERSION,
            &sig,
        );
    }

    let hash = BytesN::from_array(&env, &[42u8; 32]);
    let sig2 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user2,
        202401u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(
        &user2,
        &202401u64,
        &archetype,
        &hash,
        &CURRENT_PAYLOAD_VERSION,
        &sig2,
    );

    assert_eq!(client.total_revoked(), 0);

    // After one revocation it is 1.
    client.revoke_wrap(&user1, &202401u64, &reason_hash);
    assert_eq!(client.total_revoked(), 1);

    // Burn should not affect the counter.
    client.burn_wrap(&user1, &202402u64);
    assert_eq!(client.total_revoked(), 1);

    // Set up another wrap to revoke for user 1 to make it 3 revocations across 2 users
    let hash = BytesN::from_array(&env, &[43u8; 32]);
    let sig3 = sign_payload(
        &env,
        &signing_key,
        &contract_id,
        &user1,
        202403u64,
        &archetype,
        &hash,
    );
    client.mint_wrap(
        &user1,
        &202403u64,
        &archetype,
        &hash,
        &CURRENT_PAYLOAD_VERSION,
        &sig3,
    );

    client.revoke_wrap(&user1, &202403u64, &reason_hash);
    assert_eq!(client.total_revoked(), 2);

    // Revoke the wrap for user 2
    client.revoke_wrap(&user2, &202401u64, &reason_hash);

    // After three revocations across two users it is 3.
    assert_eq!(client.total_revoked(), 3);
}
