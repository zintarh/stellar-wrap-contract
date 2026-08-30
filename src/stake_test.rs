#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, TryIntoVal,
};

// ── Stake tests ─────────────────────────────────────────────────────────────

fn env_with_time() -> Env {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    env
}

#[test]
fn test_stake_basic_flow() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Initially no stake
    assert!(client.get_stake(&user).is_none());
    assert_eq!(client.total_staked(), 0);
    assert_eq!(client.get_stake_priority(&user), 0);

    // Stake 500 tokens
    client.stake(&user, &500);
    let record = client.get_stake(&user).unwrap();
    assert_eq!(record.amount, 500);
    assert_eq!(record.unstaking_at, 0);
    assert!(record.staked_at > 0);
    assert_eq!(client.total_staked(), 500);
}

#[test]
fn test_stake_multiple_times_accumulates() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &100);
    client.stake(&user, &200);
    client.stake(&user, &300);

    let record = client.get_stake(&user).unwrap();
    assert_eq!(record.amount, 600);
    assert_eq!(client.total_staked(), 600);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_stake_below_minimum_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Default min_stake is 100, try staking 50
    client.stake(&user, &50);
}

#[test]
fn test_unstake_and_withdraw_flow() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &1000);

    // Initiate unstake
    client.unstake(&user);
    let record = client.get_stake(&user).unwrap();
    assert_eq!(record.amount, 1000);
    assert!(record.unstaking_at > 0);

    // Priority should be 0 during unstaking
    assert_eq!(client.get_stake_priority(&user), 0);

    // Advance time past cooldown (default 7 days = 604800 seconds)
    env.ledger().with_mut(|li| {
        li.timestamp += 604801;
    });

    // Withdraw
    client.withdraw_stake(&user);
    assert!(client.get_stake(&user).is_none());
    assert_eq!(client.total_staked(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_withdraw_before_cooldown_fails() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &1000);
    client.unstake(&user);

    // Try to withdraw immediately (no time passed)
    client.withdraw_stake(&user);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_unstake_nonexistent_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.unstake(&user);
}

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn test_double_unstake_fails() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &500);
    client.unstake(&user);
    client.unstake(&user); // Should panic: cooldown already active
}

#[test]
fn test_stake_priority_computation() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Default config: min_stake=100, multiplier=1000bps (10%), max=5000bps (50%)
    // Stake 100 -> 1x min_stake -> priority = 1 * 1000 = 1000 bps (10%)
    client.stake(&user, &100);
    assert_eq!(client.get_stake_priority(&user), 1000);

    // Add more -> 300 total -> 3x min_stake -> 3000 bps (30%)
    client.stake(&user, &200);
    assert_eq!(client.get_stake_priority(&user), 3000);

    // Add more -> 600 total -> 6x min_stake -> capped at 5000 bps (50%)
    client.stake(&user, &300);
    assert_eq!(client.get_stake_priority(&user), 5000);
}

#[test]
fn test_stake_priority_zero_for_non_staker() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);

    assert_eq!(client.get_stake_priority(&user), 0);
}

#[test]
fn test_stake_config_defaults() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);

    let config = client.get_stake_config();
    assert_eq!(config.min_stake, 100);
    assert_eq!(config.cooldown_seconds, 7 * 24 * 60 * 60);
    assert_eq!(config.priority_multiplier_bps, 1000);
    assert_eq!(config.max_priority_bps, 5000);
}

#[test]
fn test_admin_set_stake_config() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let new_config = StakeConfig {
        min_stake: 200,
        cooldown_seconds: 3600,       // 1 hour
        priority_multiplier_bps: 500, // 5% per min_stake unit
        max_priority_bps: 3000,       // 30% max
    };
    client.set_stake_config(&new_config);

    let config = client.get_stake_config();
    assert_eq!(config.min_stake, 200);
    assert_eq!(config.cooldown_seconds, 3600);
    assert_eq!(config.priority_multiplier_bps, 500);
    assert_eq!(config.max_priority_bps, 3000);
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_invalid_stake_config_zero_min_stake_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let bad_config = StakeConfig {
        min_stake: 0,
        cooldown_seconds: 3600,
        priority_multiplier_bps: 500,
        max_priority_bps: 3000,
    };
    client.set_stake_config(&bad_config);
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_invalid_stake_config_max_bps_exceeds_10000_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    let bad_config = StakeConfig {
        min_stake: 100,
        cooldown_seconds: 3600,
        priority_multiplier_bps: 500,
        max_priority_bps: 10001,
    };
    client.set_stake_config(&bad_config);
}

#[test]
fn test_total_staked_multi_user() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    let user_c = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user_a, &100);
    client.stake(&user_b, &200);
    client.stake(&user_c, &300);

    assert_eq!(client.total_staked(), 600);

    // Unstake and withdraw one
    client.unstake(&user_b);
    env.ledger().with_mut(|li| {
        li.timestamp += 604801;
    });
    client.withdraw_stake(&user_b);

    assert_eq!(client.total_staked(), 400);
}

#[test]
fn test_discounted_fee_with_stake() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Set up a fee model so there's a non-zero fee
    let fee_params = storage_types::FeeParams {
        base_fee: 1000,
        per_kib_fee: 100,
        scale_step_kib: 1,
        max_fee: 10000,
    };
    client.set_fee_params(&fee_params);

    // No stake -> no discount
    let fee_no_stake = client.get_discounted_fee(&user);
    let raw_fee = client.current_fee();
    assert_eq!(fee_no_stake, raw_fee);

    // Stake to get ~10% discount (1000 bps priority)
    client.stake(&user, &100); // 1x min_stake -> 1000 bps = 10%
    let fee_with_stake = client.get_discounted_fee(&user);
    assert!(fee_with_stake < fee_no_stake);
}

#[test]
fn test_discounted_fee_zero_when_raw_fee_zero() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // Default fee params have base_fee=0 and per_kib_fee=0 -> fee = 0
    client.stake(&user, &500);

    let discounted = client.get_discounted_fee(&user);
    assert_eq!(discounted, 0);
}

#[test]
fn test_stake_events_emitted() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &500);

    let events = crate::test_utils::decode_events(&env);
    // Find the stake event
    let stake_events: std::vec::Vec<_> = events
        .iter()
        .filter(|(topics, _)| {
            if topics.len() >= 2 {
                let t0: Result<soroban_sdk::Symbol, _> = topics[0].try_into_val(&env);
                t0.is_ok_and(|s| s == symbol_short!("stake"))
            } else {
                false
            }
        })
        .collect();

    assert!(
        !stake_events.is_empty(),
        "Expected at least one stake event"
    );
}

#[test]
fn test_cannot_stake_during_unstaking() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &200);
    client.unstake(&user);

    // Try to stake more during unstaking period — should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.stake(&user, &100);
    }));
    assert!(result.is_err());
}

#[test]
fn test_re_stake_after_withdraw() {
    let env = env_with_time();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    // First stake
    client.stake(&user, &500);
    assert_eq!(client.get_stake(&user).unwrap().amount, 500);

    // Unstake and withdraw
    client.unstake(&user);
    env.ledger().with_mut(|li| {
        li.timestamp += 604801;
    });
    client.withdraw_stake(&user);
    assert!(client.get_stake(&user).is_none());

    // Re-stake after withdrawal
    client.stake(&user, &300);
    let record = client.get_stake(&user).unwrap();
    assert_eq!(record.amount, 300);
    assert_eq!(record.unstaking_at, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_withdraw_without_unstake_fails() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pubkey = BytesN::from_array(&env, &[1u8; 32]);
    let user = Address::generate(&env);

    client.initialize(&admin, &pubkey);
    env.mock_all_auths();

    client.stake(&user, &500);
    // Call withdraw without calling unstake first
    client.withdraw_stake(&user);
}

// ── Property tests for get_stake_priority arithmetic (#652) ─────────────────

#[cfg(test)]
mod stake_priority_prop_tests {
    extern crate std;
    use proptest::prelude::*;

    /// Pure re-implementation of the fixed arithmetic so proptest can exercise
    /// it without needing a full Soroban Env.
    fn compute_priority(amount: i128, min_stake: i128, multiplier_bps: u32, max_bps: u32) -> u32 {
        if min_stake == 0 || amount < min_stake {
            return 0;
        }
        let multiples: i128 = amount / min_stake;
        let priority: i128 = multiples.saturating_mul(multiplier_bps as i128);
        let capped: i128 = priority.min(max_bps as i128);
        capped as u32
    }

    proptest! {
        /// a) Priority is monotonically non-decreasing as `amount` increases,
        ///    across a wide i128 range including values near/above u32::MAX * min_stake.
        #[test]
        fn prop_priority_monotone(
            // min_stake in [1, 1_000] to keep the test fast
            min_stake in 1i128..=1_000i128,
            multiplier_bps in 0u32..=10_000u32,
            max_bps       in 0u32..=10_000u32,
            // amount_a in a very wide range, including near u32::MAX * min_stake
            amount_a in 0i128..=i128::MAX / 2,
        ) {
            // amount_b is any value >= amount_a (saturate so we don't overflow)
            let amount_b = amount_a.saturating_add(amount_a / 2 + 1).min(i128::MAX);

            let p_a = compute_priority(amount_a, min_stake, multiplier_bps, max_bps);
            let p_b = compute_priority(amount_b, min_stake, multiplier_bps, max_bps);

            prop_assert!(
                p_b >= p_a,
                "priority({}) = {} > priority({}) = {} — not monotone",
                amount_b, p_b, amount_a, p_a
            );
        }

        /// b) Result never exceeds max_priority_bps.
        #[test]
        fn prop_priority_never_exceeds_max(
            min_stake     in 1i128..=1_000i128,
            multiplier_bps in 0u32..=10_000u32,
            max_bps        in 0u32..=10_000u32,
            amount         in 0i128..=i128::MAX / 2,
        ) {
            let p = compute_priority(amount, min_stake, multiplier_bps, max_bps);
            prop_assert!(
                p <= max_bps,
                "priority {} exceeded max_priority_bps {}",
                p, max_bps
            );
        }

        /// c) Large stakes (well above u32::MAX * min_stake) still cap correctly —
        ///    this is the exact regression case for #652.
        #[test]
        fn prop_large_stake_caps_at_max(
            min_stake      in 1i128..=100i128,
            multiplier_bps in 1u32..=10_000u32,
            max_bps        in 1u32..=10_000u32,
        ) {
            // amount that would have overflowed the old `as u32` cast:
            // u32::MAX as i128 * min_stake + min_stake  (one past the overflow boundary)
            let huge_amount = (u32::MAX as i128 + 1) * min_stake;
            let p = compute_priority(huge_amount, min_stake, multiplier_bps, max_bps);
            prop_assert!(
                p <= max_bps,
                "large-stake priority {} exceeded max_priority_bps {}",
                p, max_bps
            );
        }
    }
}
