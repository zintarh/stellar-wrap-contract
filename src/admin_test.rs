#![cfg(test)]

use super::{StellarWrapContract, StellarWrapContractClient};
use crate::test_utils::decode_events;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, TryIntoVal};

/// Registers the contract and initializes it with a freshly generated admin.
fn setup(env: &Env) -> (StellarWrapContractClient<'_>, Address) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let admin_pubkey = BytesN::from_array(env, &[1u8; 32]);
    client.initialize(&admin, &admin_pubkey);
    (client, admin)
}

#[test]
fn test_redundant_pause_does_not_emit_an_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Genuine initial pause transition emits exactly one event.
    client.pause();
    let genuine = decode_events(&env);
    assert_eq!(
        genuine.len(),
        1,
        "a genuine pause must emit exactly one event"
    );

    // A redundant pause (already paused) must emit no additional event.
    assert!(client.is_paused());
    client.pause();
    let redundant = decode_events(&env);
    assert_eq!(
        redundant.len(),
        0,
        "a redundant pause must not emit an event"
    );
}

#[test]
fn test_redundant_unpause_does_not_emit_an_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    client.pause();
    decode_events(&env); // drain the pause event

    // Genuine unpause transition emits exactly one event.
    client.unpause();
    let genuine = decode_events(&env);
    assert_eq!(
        genuine.len(),
        1,
        "a genuine unpause must emit exactly one event"
    );

    // A redundant unpause (already unpaused) must emit no additional event.
    assert!(!client.is_paused());
    client.unpause();
    let redundant = decode_events(&env);
    assert_eq!(
        redundant.len(),
        0,
        "a redundant unpause must not emit an event"
    );
}

#[test]
fn test_pause_and_unpause_are_distinguishable_by_topic_and_carry_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    // Pause: topic is ("pause", "paused"), payload is the acting admin.
    client.pause();
    let pause_events = decode_events(&env);
    assert_eq!(pause_events.len(), 1, "expected a single pause event");
    let (pause_topics, pause_data) = pause_events.last().expect("no pause event");
    let pause_action: Symbol = pause_topics[0].try_into_val(&env).unwrap();
    let pause_direction: Symbol = pause_topics[1].try_into_val(&env).unwrap();
    let pause_admin: Address = pause_data.clone().try_into_val(&env).unwrap();
    assert_eq!(pause_action, symbol_short!("pause"));
    assert_eq!(pause_direction, symbol_short!("paused"));
    assert_eq!(pause_admin, admin);

    // Unpause: topic is ("pause", "unpaused"), payload is the acting admin.
    client.unpause();
    let unpause_events = decode_events(&env);
    assert_eq!(unpause_events.len(), 1, "expected a single unpause event");
    let (unpause_topics, unpause_data) = unpause_events.last().expect("no unpause event");
    let unpause_action: Symbol = unpause_topics[0].try_into_val(&env).unwrap();
    let unpause_direction: Symbol = unpause_topics[1].try_into_val(&env).unwrap();
    let unpause_admin: Address = unpause_data.clone().try_into_val(&env).unwrap();
    assert_eq!(unpause_action, symbol_short!("pause"));
    assert_eq!(unpause_direction, symbol_short!("unpaused"));
    assert_eq!(unpause_admin, admin);
}

#[test]
fn test_schema_version_initialized_at_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Schema version should be 1 after initialization
    let schema_version = client.schema_version();
    assert_eq!(
        schema_version, 1,
        "schema version should be 1 after initialization"
    );
}

// ─── Two-step signing key rotation tests (#1053) ───────────────────────────

extern crate std;
use std::vec;
use ed25519_dalek::{Signer, SigningKey};
use crate::signature::construct_signing_key_challenge;
use crate::ContractError;

fn sign_challenge(
    env: &Env,
    signer: &SigningKey,
    contract_id: &Address,
    nonce: u64,
) -> BytesN<64> {
    let payload = construct_signing_key_challenge(env, contract_id, nonce);
    let mut output = vec![0u8; payload.len() as usize];
    payload.copy_into_slice(&mut output);
    BytesN::from_array(env, &signer.sign(&output).to_bytes())
}

#[test]
fn test_two_step_signing_key_rotation_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[42u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    // Step 1: propose new signing key
    client.propose_signing_key(&new_pubkey);
    assert_eq!(client.get_pending_signing_key(), Some(new_pubkey.clone()));
    let nonce = client.get_signing_key_nonce();
    assert_eq!(nonce, 1);

    // Step 2: sign challenge and accept
    let signature = sign_challenge(&env, &new_signer, &client.address, nonce);
    client.accept_signing_key(&signature);

    // Verify key was updated and proposal cleared
    assert_eq!(client.get_admin_pubkey(), Some(new_pubkey.clone()));
    assert_eq!(client.get_pending_signing_key(), None);

    // Verify rotation event was emitted
    let events = decode_events(&env);
    let (topics, data) = events.last().expect("rotation event emitted");
    assert_eq!(topics[0], symbol_short!("v1"));
    assert_eq!(topics[1], symbol_short!("pubkey"));
    assert_eq!(topics[2], symbol_short!("rotate"));
    let emitted_key: BytesN<32> = data.clone().try_into_val(&env).unwrap();
    assert_eq!(emitted_key, new_pubkey);
}

#[test]
fn test_accept_signing_key_with_wrong_key_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[42u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    let wrong_signer = SigningKey::from_bytes(&[43u8; 32]);

    client.propose_signing_key(&new_pubkey);
    let nonce = client.get_signing_key_nonce();

    // Sign with the wrong key
    let wrong_signature = sign_challenge(&env, &wrong_signer, &client.address, nonce);
    let res = client.try_accept_signing_key(&wrong_signature);
    assert_eq!(res, Err(Ok(ContractError::InvalidSignature)));

    // Proposed key remains pending, active key unchanged
    assert_eq!(client.get_pending_signing_key(), Some(new_pubkey));
}

#[test]
fn test_accept_signing_key_with_invalid_signature_bytes_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[44u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    client.propose_signing_key(&new_pubkey);

    let corrupt_signature = BytesN::from_array(&env, &[0u8; 64]);
    let res = client.try_accept_signing_key(&corrupt_signature);
    assert_eq!(res, Err(Ok(ContractError::InvalidSignature)));
}

#[test]
fn test_accept_signing_key_with_wrong_nonce_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[45u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    client.propose_signing_key(&new_pubkey);
    let nonce = client.get_signing_key_nonce();

    // Sign challenge with a wrong nonce
    let signature = sign_challenge(&env, &new_signer, &client.address, nonce + 1);
    let res = client.try_accept_signing_key(&signature);
    assert_eq!(res, Err(Ok(ContractError::InvalidSignature)));
}

#[test]
fn test_accept_signing_key_with_wrong_contract_id_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[46u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    client.propose_signing_key(&new_pubkey);
    let nonce = client.get_signing_key_nonce();

    let other_contract = Address::generate(&env);
    let signature = sign_challenge(&env, &new_signer, &other_contract, nonce);
    let res = client.try_accept_signing_key(&signature);
    assert_eq!(res, Err(Ok(ContractError::InvalidSignature)));
}

#[test]
fn test_propose_signing_key_when_proposal_exists_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let key1 = BytesN::from_array(&env, &[11u8; 32]);
    let key2 = BytesN::from_array(&env, &[12u8; 32]);

    client.propose_signing_key(&key1);
    let res = client.try_propose_signing_key(&key2);
    assert_eq!(res, Err(Ok(ContractError::AdminTransferProposalExists)));
}

#[test]
fn test_propose_signing_key_all_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let zero_key = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_propose_signing_key(&zero_key);
    assert_eq!(res, Err(Ok(ContractError::InvalidAdminPubKey)));
}

#[test]
fn test_accept_signing_key_when_no_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let dummy_sig = BytesN::from_array(&env, &[1u8; 64]);
    let res = client.try_accept_signing_key(&dummy_sig);
    assert_eq!(res, Err(Ok(ContractError::NoAdminTransferProposal)));
}

#[test]
fn test_cancel_proposed_signing_key() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let new_signer = SigningKey::from_bytes(&[47u8; 32]);
    let new_pubkey = BytesN::from_array(&env, &new_signer.verifying_key().to_bytes());

    client.propose_signing_key(&new_pubkey);
    assert_eq!(client.get_pending_signing_key(), Some(new_pubkey));

    client.cancel_proposed_signing_key();
    assert_eq!(client.get_pending_signing_key(), None);

    // Accept fails after cancellation
    let dummy_sig = BytesN::from_array(&env, &[1u8; 64]);
    let res = client.try_accept_signing_key(&dummy_sig);
    assert_eq!(res, Err(Ok(ContractError::NoAdminTransferProposal)));
}

#[test]
fn test_cancel_signing_key_when_no_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let res = client.try_cancel_proposed_signing_key();
    assert_eq!(res, Err(Ok(ContractError::NoAdminTransferProposal)));
}

#[test]
fn test_signing_key_rotation_blocked_when_timelock_enabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    // Enable timelock with min delay (1 hour)
    client.enable_timelock(&3600);

    let new_key = BytesN::from_array(&env, &[48u8; 32]);
    let dummy_sig = BytesN::from_array(&env, &[1u8; 64]);

    // Direct propose, accept, and update calls are all rejected
    assert_eq!(
        client.try_propose_signing_key(&new_key),
        Err(Ok(ContractError::TimelockRequired))
    );
    assert_eq!(
        client.try_accept_signing_key(&dummy_sig),
        Err(Ok(ContractError::TimelockRequired))
    );
    assert_eq!(
        client.try_update_admin_pubkey(&new_key),
        Err(Ok(ContractError::TimelockRequired))
    );
}
