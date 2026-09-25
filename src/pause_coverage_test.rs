#![cfg(test)]

extern crate std;

use super::*;
use crate::signature::{construct_inbound_bridge_payload, construct_mint_payload};
use crate::storage_types::StakeConfig;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env, Symbol};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup_test_env<'a>(
    env: &'a Env,
) -> (StellarWrapContractClient<'a>, Address, Address, SigningKey) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);
    let relayer = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    (client, admin, relayer, signing_key)
}

fn sign_mint_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = construct_mint_payload(env, contract, user, period, archetype, data_hash, 1);
    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);
    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

fn sign_inbound_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = construct_inbound_bridge_payload(
        env,
        contract,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
    );
    let mut out = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut out[..len]);
    let signature = signer.sign(&out[..len]);
    BytesN::from_array(env, &signature.to_bytes())
}

#[test]
fn test_pause_blocks_all_require_not_paused_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, relayer, signing_key) = setup_test_env(&env);

    // Set up bridge chain + relayers for bridge tests
    let chain_id = 1u32;
    client.set_chain_status(&chain_id, &true);
    let relayer_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let mut relayers = soroban_sdk::Vec::new(&env);
    relayers.push_back(relayer_pubkey);
    client.set_bridge_relayers(&chain_id, &relayers, &1);

    // Mint a wrap used by transition / expire / bridge tests
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    // Set up stake config
    let config = StakeConfig {
        min_stake: 100,
        cooldown_seconds: 7 * 24 * 60 * 60,
        priority_multiplier_bps: 1000,
        max_priority_bps: 5000,
    };
    client.set_stake_config(&config);

    // Pause the contract
    client.pause();

    // ── mint_wrap ────────────────────────────────────────────────────────
    let sig2 = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig2);
    }));
    assert!(result.is_err(), "mint_wrap must be blocked while paused");

    // ── mint_wrap_batch ──────────────────────────────────────────────────
    let mut items = soroban_sdk::Vec::new(&env);
    let batch_item = storage_types::BatchWrapItem {
        user: user.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version: 1,
        signature: sig2.clone(),
    };
    items.push_back(batch_item);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.mint_wrap_batch(&items, &None);
    }));
    assert!(
        result.is_err(),
        "mint_wrap_batch must be blocked while paused"
    );

    // ── transition_wrap_state ────────────────────────────────────────────
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.transition_wrap_state(&user, &period, &WrapState::Revoked);
    }));
    assert!(
        result.is_err(),
        "transition_wrap_state must be blocked while paused"
    );

    // ── expire_wrap ─────────────────────────────────────────────────────
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.expire_wrap(&user, &period);
    }));
    assert!(result.is_err(), "expire_wrap must be blocked while paused");

    // ── bridge_wrap_out ─────────────────────────────────────────────────
    let recipient_bytes = Bytes::from_array(&env, b"recipient");
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_out(&user, &chain_id, &recipient_bytes, &period);
    }));
    assert!(
        result.is_err(),
        "bridge_wrap_out must be blocked while paused"
    );

    // ── bridge_wrap_refund ──────────────────────────────────────────────
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_refund(&0);
    }));
    assert!(
        result.is_err(),
        "bridge_wrap_refund must be blocked while paused"
    );

    // ── bridge_wrap_in ──────────────────────────────────────────────────
    let sig_in = sign_inbound_payload(
        &env,
        &signing_key,
        &client.address,
        chain_id,
        500u64,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    let mut signatures = soroban_sdk::Vec::new(&env);
    signatures.push_back(sig_in);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.bridge_wrap_in(
            &chain_id,
            &500u64,
            &user,
            &period,
            &archetype,
            &data_hash,
            &signatures,
        );
    }));
    assert!(
        result.is_err(),
        "bridge_wrap_in must be blocked while paused"
    );

    // ── stake ───────────────────────────────────────────────────────────
    let stake_user = Address::generate(&env);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.stake(&stake_user, &500);
    }));
    assert!(result.is_err(), "stake must be blocked while paused");

    // ── unstake ─────────────────────────────────────────────────────────
    let unstake_user = Address::generate(&env);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.unstake(&unstake_user);
    }));
    assert!(result.is_err(), "unstake must be blocked while paused");

    // ── withdraw_stake ──────────────────────────────────────────────────
    let withdraw_user = Address::generate(&env);
    let result = catch_unwind(AssertUnwindSafe(|| {
        client.withdraw_stake(&withdraw_user);
    }));
    assert!(
        result.is_err(),
        "withdraw_stake must be blocked while paused"
    );
}

#[test]
fn test_pause_allows_documented_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    // Mint a wrap for burn / extend_ttl tests
    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);
    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);

    client.pause();

    // ── Entrypoints intentionally allowed while paused ──────────────────
    //
    // The following mutating entrypoints do NOT call require_not_paused.
    // They remain callable when the contract is paused. Each is listed with
    // the documented reason:
    //
    // - initialize:        One-time deployment setup; pause is irrelevant before init.
    // - update_admin:      Admin rotation must always be possible for security.
    // - set_transfer_fee:  Admin fee configuration; operational necessity.
    // - clear_transfer_fee: Admin fee configuration; operational necessity.
    // - pause / unpause:   Meta-control entrypoints; unpause must work when paused.
    // - migrate / upgrade: Admin maintenance; must remain available.
    // - propose_admin / accept_admin / cancel_proposed_admin: Admin rotation flow.
    // - set_name / set_symbol: Admin metadata configuration.
    // - transfer_wrap:    User-initiated wrap transfer; pause halts mint/bridge/stake only.
    // - backfill_wrap_periods: Admin data repair; must remain possible.
    // - set_expiration_duration: Admin configuration.
    // - extend_ttl:       Permissionless storage preservation; must remain available.
    // - renew_all_ttls:   Admin storage preservation; must remain available.
    // - set_alias_hash:   User preference update; must remain available.
    // - opt_out / opt_in: User preference update; must remain available.
    // - revoke_wrap:      Admin revocation; must remain available for compliance.
    // - burn_wrap:        User-initiated irreversible burn; must remain available.
    // - set_fee_params:   Admin accounting configuration.
    // - set_whitelist_root / clear_whitelist_root: Admin whitelist configuration.
    // - enable_timelock / timelock_schedule / timelock_execute / timelock_cancel:
    //                     Timelock operations; must remain available.
    // - set_bridge_relayers / set_chain_status: Admin bridge configuration.
    // - create_admin_proposal / vote_admin_proposal / execute_admin_proposal /
    //   cancel_admin_proposal: DAO governance; must remain available.
    // - set_stake_config: Admin staking configuration.
    // ─────────────────────────────────────────────────────────────────────

    // burn_wrap — user burn should still work while paused
    let burn_user = Address::generate(&env);
    let burn_period = 202608u64;
    let burn_sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &burn_user,
        burn_period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(
        &burn_user,
        &burn_period,
        &archetype,
        &data_hash,
        &1,
        &burn_sig,
    );
    client.burn_wrap(&burn_user, &burn_period);
    assert!(client.get_wrap(&burn_user, &burn_period).is_none());

    // set_alias_hash — user preference
    let alias_user = Address::generate(&env);
    client.set_alias_hash(&alias_user, &BytesN::from_array(&env, &[99u8; 32]));

    // opt_out / opt_in — user preference
    let opt_user = Address::generate(&env);
    client.opt_out(&opt_user);
    assert!(client.is_opted_out(&opt_user));
    client.opt_in(&opt_user);
    assert!(!client.is_opted_out(&opt_user));

    // extend_ttl — permissionless TTL renewal
    client.extend_ttl(&user, &period);

    // set_name / set_symbol — admin metadata
    client.set_name(&symbol_short!("Test"));
    client.set_symbol(&symbol_short!("TST"));

    // set_fee_params — admin accounting
    client.set_fee_params(&storage_types::FeeParams {
        base_fee: 100,
        per_kib_fee: 10,
        scale_step_kib: 1,
        max_fee: 1000,
    });

    // set_whitelist_root — admin whitelist
    client.set_whitelist_root(&BytesN::from_array(&env, &[1u8; 32]));

    // enable_timelock — admin timelock config
    client.enable_timelock(&3600);

    // set_bridge_relayers — admin bridge config
    let chain_2 = 2u32;
    client.set_chain_status(&chain_2, &true);
    let mut relayers2 = soroban_sdk::Vec::new(&env);
    relayers2.push_back(BytesN::from_array(&env, &[9u8; 32]));
    client.set_bridge_relayers(&chain_2, &relayers2, &1);

    // set_stake_config — admin staking config
    client.set_stake_config(&StakeConfig {
        min_stake: 200,
        cooldown_seconds: 86400,
        priority_multiplier_bps: 10000,
        max_priority_bps: 10000,
    });
}

#[test]
fn test_unpause_restores_blocked_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &[42u8; 32]);

    // Pause, verify mint is blocked, then unpause and verify it works
    client.pause();

    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    let paused_result = catch_unwind(AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period, &archetype, &data_hash, &1, &sig);
    }));
    assert!(
        paused_result.is_err(),
        "mint_wrap must be blocked while paused"
    );

    client.unpause();

    let sig2 = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period + 1,
        &archetype,
        &data_hash,
    );
    let unpaused_result = catch_unwind(AssertUnwindSafe(|| {
        client.mint_wrap(&user, &period + 1, &archetype, &data_hash, &1, &sig2);
    }));
    assert!(
        unpaused_result.is_ok(),
        "mint_wrap must succeed after unpause"
    );

    // Pause again and verify a different blocked entrypoint
    client.pause();

    let stake_user = Address::generate(&env);
    let stake_result = catch_unwind(AssertUnwindSafe(|| {
        client.stake(&stake_user, &500);
    }));
    assert!(stake_result.is_err(), "stake must be blocked while paused");

    client.unpause();

    let unstake_result = catch_unwind(AssertUnwindSafe(|| {
        client.unstake(&stake_user);
    }));
    assert!(
        unstake_result.is_err(),
        "unstake should fail with StakeNotFound, not Paused, after unpause"
    );
}
