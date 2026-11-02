#![cfg(test)]
//! Tests demonstrating fine-grained pause flag functionality.
//!
//! This is part of the phased rollout of issue #649 - replacing the single
//! boolean Paused flag with granular PauseFlags for selective operation control.

use crate::{
    storage_types::{DataKey, PauseFlags},
    testutils::*,
    StellarWrapContract, StellarWrapContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Demonstrates that the mint flag can be read and written independently.
#[test]
fn test_pause_flags_storage() {
    let env = Env::default();
    let (client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    // Initially, all flags should be off (none() is the default)
    let initial_flags = crate::admin::read_pause_flags(&env);
    assert!(!initial_flags.mint);
    assert!(!initial_flags.bridge);
    assert!(!initial_flags.is_any_paused());

    // Set mint flag to paused, leave others alone
    let mut flags = PauseFlags::none();
    flags.mint = true;
    crate::admin::write_pause_flags(&env, &flags);

    // Read back and verify
    let stored_flags = crate::admin::read_pause_flags(&env);
    assert!(stored_flags.mint);
    assert!(!stored_flags.bridge);
    assert!(stored_flags.is_any_paused());
}

/// Demonstrates that pausing mint via the flag blocks mint operations.
#[test]
#[should_panic(expected = "Paused")]
fn test_mint_pause_flag_blocks_mint() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _relayer, signing_key) = setup_test_env(&env);

    let user = Address::generate(&env);
    let period = 202607u64;
    let archetype = soroban_sdk::symbol_short!("arch");
    let data_hash = soroban_sdk::BytesN::from_array(&env, &[42u8; 32]);

    // Pause only mint operations
    let mut flags = PauseFlags::none();
    flags.mint = true;
    crate::admin::write_pause_flags(&env, &flags);

    // This should panic because mint is paused
    let sig = sign_mint_payload(
        &env,
        &signing_key,
        &client.address,
        &user,
        period,
        &archetype,
        &data_hash,
    );
    
    // Replace the require_not_paused check with require_mint_not_paused in mint.rs
    // For now, this test documents the intended behavior
    crate::admin::require_mint_not_paused(&env);
}

/// Demonstrates that pausing bridge via the flag would block bridge operations
/// while leaving other operations functional.
#[test]
#[should_panic(expected = "Paused")]
fn test_bridge_pause_flag_blocks_bridge() {
    let env = Env::default();
    env.mock_all_auths();
    let (_client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    // Pause only bridge operations
    let mut flags = PauseFlags::none();
    flags.bridge = true;
    crate::admin::write_pause_flags(&env, &flags);

    // This should panic because bridge is paused
    crate::admin::require_bridge_not_paused(&env);
}

/// Demonstrates the key behavior: pausing mint does NOT affect bridge checks.
#[test]
fn test_selective_pause_mint_allows_bridge() {
    let env = Env::default();
    let (_client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    // Pause only mint operations
    let mut flags = PauseFlags::none();
    flags.mint = true;
    crate::admin::write_pause_flags(&env, &flags);

    // Bridge check should pass (not panicking means success)
    crate::admin::require_bridge_not_paused(&env);
}

/// Demonstrates that when no flags are set, all checks pass.
#[test]
fn test_no_pause_flags_allows_all() {
    let env = Env::default();
    let (_client, _admin, _relayer, _signing_key) = setup_test_env(&env);

    // Explicitly set no flags
    crate::admin::write_pause_flags(&env, &PauseFlags::none());

    // All checks should pass
    crate::admin::require_mint_not_paused(&env);
    crate::admin::require_bridge_not_paused(&env);
}
