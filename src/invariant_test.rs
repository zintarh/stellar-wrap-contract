#![cfg(test)]
extern crate std;

use crate::{StellarWrapContract, StellarWrapContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Symbol};
use crate::test_utils::{assert_user_invariants, sign_payload_versioned};
use ed25519_dalek::SigningKey;
use crate::mint::CURRENT_PAYLOAD_VERSION;

fn setup() -> (Env, StellarWrapContractClient<'static>, Address, SigningKey) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin_signing_key = SigningKey::from_bytes(&[1u8; 32]);
    let admin_pubkey_bytes = admin_signing_key.verifying_key().to_bytes();
    let admin_pubkey = BytesN::from_array(&env, &admin_pubkey_bytes);
    
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    (env, client, admin, admin_signing_key)
}

fn mint_test_wrap(
    env: &Env,
    client: &StellarWrapContractClient<'static>,
    admin_signing_key: &SigningKey,
    user: &Address,
    period: u64,
) {
    let archetype = Symbol::new(env, "TEST");
    let data_hash = BytesN::from_array(env, &[1u8; 32]);
    let signature = sign_payload_versioned(
        env,
        admin_signing_key,
        &client.address,
        user,
        period,
        &archetype,
        &data_hash,
        CURRENT_PAYLOAD_VERSION,
    );
    client.mint_wrap(
        user,
        &period,
        &archetype,
        &data_hash,
        &CURRENT_PAYLOAD_VERSION,
        &signature,
    );
}

#[test]
fn test_healthy_user_invariants() {
    let (env, client, _admin, admin_key) = setup();
    let user = Address::generate(&env);

    // Initial state: no wraps
    assert_user_invariants(&env, &client.address, &user);

    // Mint one wrap
    mint_test_wrap(&env, &client, &admin_key, &user, 202401);
    assert_user_invariants(&env, &client.address, &user);

    // Mint second wrap
    mint_test_wrap(&env, &client, &admin_key, &user, 202402);
    assert_user_invariants(&env, &client.address, &user);
}

#[test]
fn test_post_revoke_invariants() {
    let (env, client, _admin, admin_key) = setup();
    let user = Address::generate(&env);

    mint_test_wrap(&env, &client, &admin_key, &user, 202401);
    mint_test_wrap(&env, &client, &admin_key, &user, 202402);

    // Revoke first wrap
    let reason_hash = BytesN::from_array(&env, &[9u8; 32]);
    client.revoke_wrap(&user, &202401, &reason_hash);
    
    // Check invariants after revoke
    assert_user_invariants(&env, &client.address, &user);

    // Revoke second wrap (now empty)
    client.revoke_wrap(&user, &202402, &reason_hash);
    
    // Check invariants after all revoked
    assert_user_invariants(&env, &client.address, &user);
}

#[test]
fn test_post_burn_invariants() {
    let (env, client, _admin, admin_key) = setup();
    let user = Address::generate(&env);

    mint_test_wrap(&env, &client, &admin_key, &user, 202401);
    mint_test_wrap(&env, &client, &admin_key, &user, 202402);

    // Burn first wrap
    client.burn_wrap(&user, &202401);
    
    // Check invariants after burn
    assert_user_invariants(&env, &client.address, &user);

    // Burn second wrap (now empty)
    client.burn_wrap(&user, &202402);
    
    // Check invariants after all burned
    assert_user_invariants(&env, &client.address, &user);
}

#[test]
fn test_post_bridge_in_invariants() {
    let (env, client, _admin, admin_key) = setup();
    let user = Address::generate(&env);

    let relayer = Address::generate(&env);
    client.set_bridge_relayer(&relayer);
    client.set_chain_status(&1, &true);

    let archetype = Symbol::new(&env, "TEST");
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);

    client.bridge_wrap_in(&1, &100, &user, &202405, &archetype, &data_hash);
    
    // Check invariants after bridge in
    assert_user_invariants(&env, &client.address, &user);
    
    mint_test_wrap(&env, &client, &admin_key, &user, 202406);
    assert_user_invariants(&env, &client.address, &user);
}

#[test]
fn test_many_periods_bound() {
    let (env, client, _admin, admin_key) = setup();
    // increase budget to allow minting 101 wraps
    env.budget().reset_unlimited();

    let user = Address::generate(&env);

    // Mint 101 wraps to exceed MAX_QUERY_RESULTS (100)
    for i in 0..101 {
        let year = 2024 + (i / 12);
        let month = (i % 12) + 1;
        let period = year * 100 + month;
        mint_test_wrap(&env, &client, &admin_key, &user, period);
    }
    
    // Check invariants, this should bound the loop and not panic from budget
    assert_user_invariants(&env, &client.address, &user);
}
