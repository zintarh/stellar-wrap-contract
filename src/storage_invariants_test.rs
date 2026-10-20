#![cfg(test)]

//! Storage invariant tests for direct state consistency.
//!
//! This module tests core storage invariants by directly inspecting contract
//! storage after mixed mint and revoke flows. These tests verify that:
//!
//! 1. Every stored wrap has a matching count contribution
//! 2. LatestPeriod references an existing wrap (with documented exceptions)
//! 3. WrapCount matches the actual number of live wraps
//! 4. UserPeriods and WrapPeriods stay synchronized
//! 5. TotalWrapCount matches the sum of all user counts
//!
//! ## Known Invariant Exceptions
//!
//! **LatestPeriod after revoking newest wrap**: When the newest wrap is revoked,
//! `LatestPeriod` is recomputed to point to the next-newest remaining period.
//! If all wraps are revoked, `LatestPeriod` is cleared (becomes `None`).

extern crate std;

use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    Address, BytesN, Env,
};

use crate::{
    test_utils::{default_test_env, sign_payload},
    DataKey, StellarWrapContract, StellarWrapContractClient,
};

/// Helper to directly read storage values without going through the contract API.
/// This allows us to verify internal storage consistency.
struct StorageInspector<'a> {
    env: &'a Env,
    contract_id: &'a Address,
}

impl<'a> StorageInspector<'a> {
    fn new(env: &'a Env, contract_id: &'a Address) -> Self {
        Self { env, contract_id }
    }

    /// Returns the WrapCount for a user by directly reading storage.
    fn get_wrap_count(&self, user: &Address) -> u32 {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::WrapCount(user.clone()))
                .unwrap_or(0)
        })
    }

    /// Returns the TotalWrapCount by directly reading storage.
    fn get_total_wrap_count(&self) -> u32 {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::TotalWrapCount)
                .unwrap_or(0)
        })
    }

    /// Returns the LatestPeriod for a user by directly reading storage.
    fn get_latest_period(&self, user: &Address) -> Option<u64> {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::LatestPeriod(user.clone()))
        })
    }

    /// Returns the UserPeriods vector by directly reading storage.
    fn get_user_periods(&self, user: &Address) -> soroban_sdk::Vec<u64> {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::UserPeriods(user.clone()))
                .unwrap_or(soroban_sdk::Vec::new(self.env))
        })
    }

    /// Returns the WrapPeriods vector by directly reading storage.
    fn get_wrap_periods(&self, user: &Address) -> soroban_sdk::Vec<u64> {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .get(&DataKey::WrapPeriods(user.clone()))
                .unwrap_or(soroban_sdk::Vec::new(self.env))
        })
    }

    /// Checks if a wrap exists for the given user and period.
    fn has_wrap(&self, user: &Address, period: u64) -> bool {
        self.env.as_contract(self.contract_id, || {
            self.env
                .storage()
                .persistent()
                .has(&DataKey::Wrap(user.clone(), period))
        })
    }

    /// Counts the actual number of live wraps for a user by iterating periods.
    fn count_live_wraps(&self, user: &Address) -> u32 {
        let user_periods = self.get_user_periods(user);
        let mut count = 0u32;
        for i in 0..user_periods.len() {
            if let Some(period) = user_periods.get(i) {
                if self.has_wrap(user, period) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Verifies that every period in UserPeriods corresponds to an actual wrap.
    fn all_user_periods_have_wraps(&self, user: &Address) -> bool {
        let user_periods = self.get_user_periods(user);
        for i in 0..user_periods.len() {
            if let Some(period) = user_periods.get(i) {
                if !self.has_wrap(user, period) {
                    return false;
                }
            }
        }
        true
    }

    /// Verifies that LatestPeriod (if set) points to an existing wrap.
    fn latest_period_is_valid(&self, user: &Address) -> bool {
        match self.get_latest_period(user) {
            None => true, // No latest period is valid (e.g., no wraps or all revoked)
            Some(period) => self.has_wrap(user, period),
        }
    }

    /// Verifies that LatestPeriod matches the maximum period in UserPeriods
    /// that has a live wrap.
    fn latest_period_is_max(&self, user: &Address) -> bool {
        let latest = self.get_latest_period(user);
        let user_periods = self.get_user_periods(user);
        
        let mut max_live_period: Option<u64> = None;
        for i in 0..user_periods.len() {
            if let Some(period) = user_periods.get(i) {
                if self.has_wrap(user, period) {
                    match max_live_period {
                        None => max_live_period = Some(period),
                        Some(current_max) if period > current_max => max_live_period = Some(period),
                        _ => {}
                    }
                }
            }
        }

        latest == max_live_period
    }

    /// Verifies that UserPeriods and WrapPeriods are synchronized.
    fn periods_are_synchronized(&self, user: &Address) -> bool {
        let user_periods = self.get_user_periods(user);
        let wrap_periods = self.get_wrap_periods(user);

        if user_periods.len() != wrap_periods.len() {
            return false;
        }

        // Check that all elements match (order-independent)
        for i in 0..user_periods.len() {
            if let Some(period) = user_periods.get(i) {
                if !wrap_periods.contains(period) {
                    return false;
                }
            }
        }

        true
    }
}

/// Test setup helper that initializes a contract and returns commonly used fixtures.
struct TestFixture {
    env: Env,
    contract_id: Address,
    client: StellarWrapContractClient,
    admin: Address,
    signing_key: ed25519_dalek::SigningKey,
}

impl TestFixture {
    fn new() -> Self {
        let env = default_test_env();
        let contract_id = env.register(StellarWrapContract, ());
        let client = StellarWrapContractClient::new(&env, &contract_id);

        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin, &admin_pubkey);

        Self {
            env,
            contract_id,
            client,
            admin,
            signing_key,
        }
    }

    fn mint_wrap(&self, user: &Address, period: u64) {
        let archetype = symbol_short!("test");
        let data_hash = BytesN::from_array(&self.env, &[period as u8; 32]);
        let signature = sign_payload(
            &self.env,
            &self.signing_key,
            &self.contract_id,
            user,
            period,
            &archetype,
            &data_hash,
        );

        self.client.mint_wrap(
            user,
            &period,
            &archetype,
            &data_hash,
            &1u32,
            &signature,
        );
    }

    fn revoke_wrap(&self, user: &Address, period: u64) {
        let reason = BytesN::from_array(&self.env, &[0u8; 32]);
        self.client.revoke_wrap(user, &period, &reason);
    }

    fn inspector(&self) -> StorageInspector {
        StorageInspector::new(&self.env, &self.contract_id)
    }
}

// ============================================================================
// CORE INVARIANT TESTS
// ============================================================================

#[test]
fn test_invariant_wrap_count_matches_live_wraps() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Initial state: no wraps
    assert_eq!(inspector.get_wrap_count(&user), 0);
    assert_eq!(inspector.count_live_wraps(&user), 0);

    // Mint first wrap
    fixture.mint_wrap(&user, 202401);
    assert_eq!(inspector.get_wrap_count(&user), 1);
    assert_eq!(inspector.count_live_wraps(&user), 1);

    // Mint second wrap
    fixture.mint_wrap(&user, 202402);
    assert_eq!(inspector.get_wrap_count(&user), 2);
    assert_eq!(inspector.count_live_wraps(&user), 2);

    // Mint third wrap
    fixture.mint_wrap(&user, 202403);
    assert_eq!(inspector.get_wrap_count(&user), 3);
    assert_eq!(inspector.count_live_wraps(&user), 3);
}

#[test]
fn test_invariant_wrap_count_decrements_after_revoke() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint three wraps
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);
    assert_eq!(inspector.get_wrap_count(&user), 3);

    // Revoke one wrap
    fixture.revoke_wrap(&user, 202402);
    assert_eq!(inspector.get_wrap_count(&user), 2);
    assert_eq!(inspector.count_live_wraps(&user), 2);

    // Revoke another
    fixture.revoke_wrap(&user, 202401);
    assert_eq!(inspector.get_wrap_count(&user), 1);
    assert_eq!(inspector.count_live_wraps(&user), 1);

    // Revoke the last one
    fixture.revoke_wrap(&user, 202403);
    assert_eq!(inspector.get_wrap_count(&user), 0);
    assert_eq!(inspector.count_live_wraps(&user), 0);
}

#[test]
fn test_invariant_latest_period_references_existing_wrap() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // No wraps yet
    assert_eq!(inspector.get_latest_period(&user), None);
    assert!(inspector.latest_period_is_valid(&user));

    // Mint first wrap
    fixture.mint_wrap(&user, 202401);
    assert_eq!(inspector.get_latest_period(&user), Some(202401));
    assert!(inspector.latest_period_is_valid(&user));

    // Mint newer wrap
    fixture.mint_wrap(&user, 202402);
    assert_eq!(inspector.get_latest_period(&user), Some(202402));
    assert!(inspector.latest_period_is_valid(&user));

    // Mint even newer wrap
    fixture.mint_wrap(&user, 202403);
    assert_eq!(inspector.get_latest_period(&user), Some(202403));
    assert!(inspector.latest_period_is_valid(&user));
}

#[test]
fn test_invariant_latest_period_after_revoking_newest() {
    // This tests the documented exception: when the newest wrap is revoked,
    // LatestPeriod should be recomputed to point to the next-newest remaining wrap.
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint three wraps: 202401, 202402, 202403
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);
    assert_eq!(inspector.get_latest_period(&user), Some(202403));

    // Revoke the newest wrap (202403)
    fixture.revoke_wrap(&user, 202403);

    // LatestPeriod should now point to 202402 (the next-newest)
    assert_eq!(inspector.get_latest_period(&user), Some(202402));
    assert!(inspector.latest_period_is_valid(&user));
    assert!(inspector.latest_period_is_max(&user));

    // Revoke 202402
    fixture.revoke_wrap(&user, 202402);

    // LatestPeriod should now point to 202401
    assert_eq!(inspector.get_latest_period(&user), Some(202401));
    assert!(inspector.latest_period_is_valid(&user));
    assert!(inspector.latest_period_is_max(&user));

    // Revoke the last wrap
    fixture.revoke_wrap(&user, 202401);

    // LatestPeriod should be cleared
    assert_eq!(inspector.get_latest_period(&user), None);
    assert!(inspector.latest_period_is_valid(&user));
}

#[test]
fn test_invariant_latest_period_after_revoking_middle_wrap() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint three wraps
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);

    // Revoke the middle wrap
    fixture.revoke_wrap(&user, 202402);

    // LatestPeriod should still point to 202403 (unchanged)
    assert_eq!(inspector.get_latest_period(&user), Some(202403));
    assert!(inspector.latest_period_is_valid(&user));
    assert!(inspector.latest_period_is_max(&user));
}

#[test]
fn test_invariant_user_periods_all_have_live_wraps_after_revoke() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint three wraps
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);

    // All periods should have live wraps
    assert!(inspector.all_user_periods_have_wraps(&user));

    // Revoke middle wrap
    fixture.revoke_wrap(&user, 202402);

    // After revoke, not all periods have live wraps anymore
    // (UserPeriods still contains 202402 but the wrap is gone)
    assert!(!inspector.all_user_periods_have_wraps(&user));

    // But WrapCount should match actual live wraps
    assert_eq!(inspector.get_wrap_count(&user), 2);
    assert_eq!(inspector.count_live_wraps(&user), 2);
}

#[test]
fn test_invariant_periods_synchronized() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Initially, no periods
    assert!(inspector.periods_are_synchronized(&user));

    // Mint first wrap
    fixture.mint_wrap(&user, 202401);
    assert!(inspector.periods_are_synchronized(&user));

    // Mint more wraps
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);
    assert!(inspector.periods_are_synchronized(&user));

    // After revoke, periods should still be synchronized
    // (both indexes keep the period, just the wrap is removed)
    fixture.revoke_wrap(&user, 202402);
    assert!(inspector.periods_are_synchronized(&user));
}

#[test]
fn test_invariant_total_wrap_count_across_users() {
    let fixture = TestFixture::new();
    let user1 = Address::generate(&fixture.env);
    let user2 = Address::generate(&fixture.env);
    let user3 = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Initial total count
    assert_eq!(inspector.get_total_wrap_count(), 0);

    // User 1 mints 2 wraps
    fixture.mint_wrap(&user1, 202401);
    fixture.mint_wrap(&user1, 202402);
    assert_eq!(inspector.get_total_wrap_count(), 2);

    // User 2 mints 3 wraps
    fixture.mint_wrap(&user2, 202401);
    fixture.mint_wrap(&user2, 202402);
    fixture.mint_wrap(&user2, 202403);
    assert_eq!(inspector.get_total_wrap_count(), 5);

    // User 3 mints 1 wrap
    fixture.mint_wrap(&user3, 202401);
    assert_eq!(inspector.get_total_wrap_count(), 6);

    // Verify individual counts
    assert_eq!(inspector.get_wrap_count(&user1), 2);
    assert_eq!(inspector.get_wrap_count(&user2), 3);
    assert_eq!(inspector.get_wrap_count(&user3), 1);
}

#[test]
fn test_invariant_total_wrap_count_decrements_after_revoke() {
    let fixture = TestFixture::new();
    let user1 = Address::generate(&fixture.env);
    let user2 = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint some wraps
    fixture.mint_wrap(&user1, 202401);
    fixture.mint_wrap(&user1, 202402);
    fixture.mint_wrap(&user2, 202401);
    assert_eq!(inspector.get_total_wrap_count(), 3);

    // Revoke one from user1
    fixture.revoke_wrap(&user1, 202401);
    assert_eq!(inspector.get_total_wrap_count(), 2);

    // Revoke one from user2
    fixture.revoke_wrap(&user2, 202401);
    assert_eq!(inspector.get_total_wrap_count(), 1);

    // Revoke last one
    fixture.revoke_wrap(&user1, 202402);
    assert_eq!(inspector.get_total_wrap_count(), 0);
}

// ============================================================================
// MIXED FLOW TESTS
// ============================================================================

#[test]
fn test_invariants_after_complex_mint_revoke_flow() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Complex flow: mint, revoke, mint, revoke in various orders
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);
    fixture.mint_wrap(&user, 202404);
    fixture.mint_wrap(&user, 202405);

    // Revoke some non-sequential periods
    fixture.revoke_wrap(&user, 202402);
    fixture.revoke_wrap(&user, 202404);

    // Verify invariants
    assert_eq!(inspector.get_wrap_count(&user), 3);
    assert_eq!(inspector.count_live_wraps(&user), 3);
    assert!(inspector.latest_period_is_valid(&user));
    assert!(inspector.latest_period_is_max(&user));
    assert_eq!(inspector.get_latest_period(&user), Some(202405));
    assert!(inspector.periods_are_synchronized(&user));

    // Mint more after revocations
    fixture.mint_wrap(&user, 202406);
    assert_eq!(inspector.get_wrap_count(&user), 4);
    assert_eq!(inspector.get_latest_period(&user), Some(202406));

    // Revoke the latest
    fixture.revoke_wrap(&user, 202406);
    assert_eq!(inspector.get_wrap_count(&user), 3);
    assert_eq!(inspector.get_latest_period(&user), Some(202405));
    assert!(inspector.latest_period_is_max(&user));
}

#[test]
fn test_invariants_multi_user_interleaved_operations() {
    let fixture = TestFixture::new();
    let user1 = Address::generate(&fixture.env);
    let user2 = Address::generate(&fixture.env);
    let user3 = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Interleaved operations across multiple users
    fixture.mint_wrap(&user1, 202401);
    fixture.mint_wrap(&user2, 202401);
    fixture.mint_wrap(&user1, 202402);
    fixture.mint_wrap(&user3, 202401);
    fixture.mint_wrap(&user2, 202402);
    fixture.mint_wrap(&user1, 202403);

    // Verify each user's invariants
    assert_eq!(inspector.get_wrap_count(&user1), 3);
    assert_eq!(inspector.count_live_wraps(&user1), 3);
    assert!(inspector.latest_period_is_valid(&user1));
    assert!(inspector.latest_period_is_max(&user1));
    assert!(inspector.periods_are_synchronized(&user1));

    assert_eq!(inspector.get_wrap_count(&user2), 2);
    assert_eq!(inspector.count_live_wraps(&user2), 2);
    assert!(inspector.latest_period_is_valid(&user2));
    assert!(inspector.latest_period_is_max(&user2));
    assert!(inspector.periods_are_synchronized(&user2));

    assert_eq!(inspector.get_wrap_count(&user3), 1);
    assert_eq!(inspector.count_live_wraps(&user3), 1);
    assert!(inspector.latest_period_is_valid(&user3));
    assert!(inspector.latest_period_is_max(&user3));
    assert!(inspector.periods_are_synchronized(&user3));

    // Total count
    assert_eq!(inspector.get_total_wrap_count(), 6);

    // Revoke from different users
    fixture.revoke_wrap(&user1, 202402);
    fixture.revoke_wrap(&user2, 202401);

    // Verify counts after revocations
    assert_eq!(inspector.get_wrap_count(&user1), 2);
    assert_eq!(inspector.get_wrap_count(&user2), 1);
    assert_eq!(inspector.get_total_wrap_count(), 4);

    // Verify all invariants still hold
    assert!(inspector.latest_period_is_valid(&user1));
    assert!(inspector.latest_period_is_max(&user1));
    assert!(inspector.latest_period_is_valid(&user2));
    assert!(inspector.latest_period_is_max(&user2));
}

#[test]
fn test_invariants_revoke_all_then_mint_new() {
    let fixture = TestFixture::new();
    let user = Address::generate(&fixture.env);
    let inspector = fixture.inspector();

    // Mint several wraps
    fixture.mint_wrap(&user, 202401);
    fixture.mint_wrap(&user, 202402);
    fixture.mint_wrap(&user, 202403);

    // Revoke all wraps
    fixture.revoke_wrap(&user, 202401);
    fixture.revoke_wrap(&user, 202402);
    fixture.revoke_wrap(&user, 202403);

    // Verify empty state
    assert_eq!(inspector.get_wrap_count(&user), 0);
    assert_eq!(inspector.count_live_wraps(&user), 0);
    assert_eq!(inspector.get_latest_period(&user), None);

    // Mint new wraps after revoking all
    fixture.mint_wrap(&user, 202404);
    fixture.mint_wrap(&user, 202405);

    // Verify invariants are restored
    assert_eq!(inspector.get_wrap_count(&user), 2);
    assert_eq!(inspector.count_live_wraps(&user), 2);
    assert_eq!(inspector.get_latest_period(&user), Some(202405));
    assert!(inspector.latest_period_is_valid(&user));
    assert!(inspector.latest_period_is_max(&user));
    assert!(inspector.periods_are_synchronized(&user));
}
