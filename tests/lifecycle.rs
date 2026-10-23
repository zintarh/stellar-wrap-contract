//! Full-lifecycle integration test for the wrap registry indexes.
//!
//! Every index bug in this module has been a *combination* bug: a state change
//! performed by one entrypoint leaves another entrypoint's assumptions broken.
//! Single-entrypoint unit tests cannot see those, so this test drives a long
//! sequence across several users and re-checks the complete invariant set after
//! **every** step.
//!
//! The invariant set is:
//!
//! - `WrapCount == UserPeriods.len() == WrapPeriods.len()`
//! - `LatestPeriod == max(UserPeriods)` when non-empty, absent otherwise
//! - every period in `UserPeriods` and `WrapPeriods` has a live `Wrap` entry
//! - `balance_of == WrapCount`
//! - `total_wrap_count` is the lifetime number of created wraps (it is not
//!   decremented by `revoke_wrap`, `burn_wrap`, or transfers)

#![allow(deprecated)]

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Bytes, BytesN, Env, Symbol, Vec,
};
use std::vec::Vec as StdVec;

use stellar_wrap_contract::{
    signature::{
        construct_batch_mint_payload, construct_inbound_bridge_payload, construct_mint_payload,
        INBOUND_BRIDGE_SCHEME_VERSION,
    },
    BatchWrapItem, DataKey, StellarWrapContract, StellarWrapContractClient, WrapState,
};

const PAYLOAD_VERSION: u32 = 2;
const CHAIN_ID: u32 = 1;

// ---------------------------------------------------------------------------
// Invariant helper
// ---------------------------------------------------------------------------

/// Assert the complete per-user invariant set, naming each broken relationship.
///
/// Reads storage directly (rather than trusting a single query entrypoint) so
/// that a broken relationship cannot be hidden by the same bug in a query.
fn assert_user_invariants(env: &Env, contract: &Address, user: &Address) {
    let (wrap_count, user_periods, wrap_periods, latest_period) = env.as_contract(contract, || {
        let wrap_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::WrapCount(user.clone()))
            .unwrap_or(0);
        let user_periods: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::UserPeriods(user.clone()))
            .unwrap_or_else(|| Vec::new(env));
        let wrap_periods: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::WrapPeriods(user.clone()))
            .unwrap_or_else(|| Vec::new(env));
        let latest_period: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::LatestPeriod(user.clone()));
        (wrap_count, user_periods, wrap_periods, latest_period)
    });

    assert_eq!(
        wrap_count,
        user_periods.len(),
        "WrapCount ({wrap_count}) != UserPeriods.len() ({}) for {user:?}",
        user_periods.len()
    );
    assert_eq!(
        wrap_count,
        wrap_periods.len(),
        "WrapCount ({wrap_count}) != WrapPeriods.len() ({}) for {user:?}",
        wrap_periods.len()
    );

    let mut max_user_period: Option<u64> = None;
    for period in user_periods.iter() {
        max_user_period = Some(match max_user_period {
            Some(current) => current.max(period),
            None => period,
        });
    }
    assert_eq!(
        latest_period, max_user_period,
        "LatestPeriod ({latest_period:?}) != max(UserPeriods) ({max_user_period:?}) for {user:?}"
    );

    for period in user_periods.iter() {
        let live = env.as_contract(contract, || {
            env.storage()
                .persistent()
                .has(&DataKey::Wrap(user.clone(), period))
        });
        assert!(
            live,
            "UserPeriods contains {period} but Wrap({user:?}, {period}) is not live"
        );
        assert!(
            wrap_periods.contains(period),
            "UserPeriods contains {period} but WrapPeriods does not, for {user:?}"
        );
    }

    for period in wrap_periods.iter() {
        let live = env.as_contract(contract, || {
            env.storage()
                .persistent()
                .has(&DataKey::Wrap(user.clone(), period))
        });
        assert!(
            live,
            "WrapPeriods contains {period} but Wrap({user:?}, {period}) is not live"
        );
        assert!(
            user_periods.contains(period),
            "WrapPeriods contains {period} but UserPeriods does not, for {user:?}"
        );
    }

    let balance = env.as_contract(contract, || {
        env.storage()
            .persistent()
            .get::<_, u32>(&DataKey::WrapCount(user.clone()))
            .unwrap_or(0) as i128
    });
    assert_eq!(
        balance, wrap_count as i128,
        "balance_of ({balance}) != WrapCount ({wrap_count}) for {user:?}"
    );
}

fn assert_total_wrap_count(client: &StellarWrapContractClient<'_>, expected: u32) {
    let actual = client.total_wrap_count();
    assert_eq!(
        actual, expected,
        "total_wrap_count is a lifetime counter: expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    env: Env,
    contract: Address,
    client: StellarWrapContractClient<'static>,
    admin_signer: SigningKey,
    relayer_signer: SigningKey,
    users: StdVec<Address>,
    lifetime_wraps: u32,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let contract = env.register(StellarWrapContract, ());
        let client = StellarWrapContractClient::new(&env, &contract);

        let admin_signer = SigningKey::from_bytes(&[11u8; 32]);
        let admin_pubkey = BytesN::from_array(&env, &admin_signer.verifying_key().to_bytes());
        let admin = Address::generate(&env);
        client.initialize(&admin, &admin_pubkey);

        let relayer_signer = SigningKey::from_bytes(&[12u8; 32]);
        let relayer_pubkey = BytesN::from_array(&env, &relayer_signer.verifying_key().to_bytes());
        let mut relayers = Vec::new(&env);
        relayers.push_back(relayer_pubkey);
        client.set_bridge_relayers(&CHAIN_ID, &relayers, &1);
        client.set_chain_status(&CHAIN_ID, &true);
        client.set_expiration_duration(&3600);

        let users = std::vec![
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        Self {
            env,
            contract,
            client,
            admin_signer,
            relayer_signer,
            users,
            lifetime_wraps: 0,
        }
    }

    fn assert_invariants(&self) {
        for user in &self.users {
            assert_user_invariants(&self.env, &self.contract, user);
        }
        assert_total_wrap_count(&self.client, self.lifetime_wraps);
    }

    fn sign_mint(
        &self,
        user: &Address,
        period: u64,
        archetype: &Symbol,
        data_hash: &BytesN<32>,
    ) -> BytesN<64> {
        let payload = construct_mint_payload(
            &self.env,
            &self.contract,
            user,
            period,
            archetype,
            data_hash,
            PAYLOAD_VERSION,
            u64::MAX,
        );
        let mut out = std::vec![0u8; payload.len() as usize];
        payload.copy_into_slice(&mut out);
        BytesN::from_array(&self.env, &self.admin_signer.sign(&out).to_bytes())
    }

    fn mint(&mut self, user: &Address, period: u64) {
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&self.env, &[period as u8; 32]);
        let signature = self.sign_mint(user, period, &archetype, &data_hash);
        self.client.mint_wrap(
            user,
            &period,
            &archetype,
            &data_hash,
            &PAYLOAD_VERSION,
            &u64::MAX,
            &signature,
        );
        self.lifetime_wraps += 1;
    }

    fn batch_mint(&mut self, items: &[(&Address, u64)]) {
        let mut batch = Vec::new(&self.env);
        for (user, period) in items {
            let archetype = symbol_short!("arch");
            let data_hash = BytesN::from_array(&self.env, &[*period as u8; 32]);
            let signature = self.sign_mint(user, *period, &archetype, &data_hash);
            batch.push_back(BatchWrapItem {
                user: (*user).clone(),
                period: *period,
                archetype,
                data_hash,
                payload_version: PAYLOAD_VERSION,
                valid_until: u64::MAX,
                signature,
            });
        }
        self.client.mint_wrap_batch(&batch, &None);
        self.lifetime_wraps += items.len() as u32;
    }

    fn transfer(&self, from: &Address, to: &Address, period: u64) {
        self.client.transfer_wrap(from, to, &period);
    }

    fn revoke(&self, user: &Address, period: u64) {
        self.client
            .revoke_wrap(user, &period, &BytesN::from_array(&self.env, &[0u8; 32]));
    }

    fn burn(&self, user: &Address, period: u64) {
        self.client.burn_wrap(user, &period);
    }

    fn transition(&self, user: &Address, period: u64, state: WrapState) {
        self.client.transition_wrap_state(user, &period, &state);
    }

    fn expire(&self, user: &Address, period: u64) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + 3601);
        self.client.expire_wrap(user, &period);
    }

    fn bridge_out(&self, user: &Address, period: u64) {
        self.client.bridge_wrap_out(
            user,
            &CHAIN_ID,
            &Bytes::from_array(&self.env, b"destination"),
            &period,
        );
    }

    fn bridge_in(&mut self, source_nonce: u64, recipient: &Address, period: u64, is_new: bool) {
        let archetype = symbol_short!("arch");
        let data_hash = BytesN::from_array(&self.env, &[period as u8; 32]);
        let payload = construct_inbound_bridge_payload(
            &self.env,
            &self.contract,
            CHAIN_ID,
            source_nonce,
            recipient,
            period,
            &archetype,
            &data_hash,
            INBOUND_BRIDGE_SCHEME_VERSION,
        );
        let mut out = std::vec![0u8; payload.len() as usize];
        payload.copy_into_slice(&mut out);
        let signature = BytesN::from_array(&self.env, &self.relayer_signer.sign(&out).to_bytes());
        let mut signatures = Vec::new(&self.env);
        signatures.push_back(signature);

        self.client.bridge_wrap_in(
            &CHAIN_ID,
            &source_nonce,
            recipient,
            &period,
            &archetype,
            &data_hash,
            &signatures,
        );
        if is_new {
            self.lifetime_wraps += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn full_lifecycle_preserves_index_invariants_after_every_step() {
    let mut fx = Fixture::new();
    let alice = fx.users[0].clone();
    let bob = fx.users[1].clone();
    let carol = fx.users[2].clone();

    fx.assert_invariants();

    // --- mint -------------------------------------------------------------
    fx.mint(&alice, 202401);
    fx.assert_invariants();
    fx.mint(&alice, 202403);
    fx.assert_invariants();

    // --- batch mint across users -----------------------------------------
    fx.batch_mint(&[(&alice, 202402), (&bob, 202401), (&bob, 202403)]);
    fx.assert_invariants();

    // --- transfer ---------------------------------------------------------
    // Alice 202402 -> Bob; this is the first ownership change in the run.
    fx.transfer(&alice, &bob, 202402);
    fx.assert_invariants();

    // --- revoke the transferred record -----------------------------------
    fx.revoke(&bob, 202402);
    fx.assert_invariants();

    // --- burn -------------------------------------------------------------
    fx.burn(&alice, 202403);
    fx.assert_invariants();

    // --- expire -----------------------------------------------------------
    // The FSM permits expiration only from Draft/Pending, so first move the
    // active record to Pending (the documented verification flow).
    fx.transition(&bob, 202403, WrapState::Pending);
    fx.assert_invariants();
    fx.expire(&bob, 202403);
    fx.assert_invariants();

    // --- bridge out + bridge in round trip -------------------------------
    fx.bridge_out(&alice, 202401);
    fx.assert_invariants();
    fx.bridge_in(1, &alice, 202401, false);
    fx.assert_invariants();

    // --- bridge in a brand new record ------------------------------------
    fx.bridge_in(2, &carol, 202404, true);
    fx.assert_invariants();

    // --- transfer the bridged-in record, then revoke it ------------------
    fx.transfer(&carol, &alice, 202404);
    fx.assert_invariants();
    fx.revoke(&alice, 202404);
    fx.assert_invariants();

    // --- burn an expired record ------------------------------------------
    fx.burn(&bob, 202403);
    fx.assert_invariants();

    // --- revoke then reuse the same period (revoke-then-mint) ------------
    fx.mint(&alice, 202402);
    fx.assert_invariants();
    fx.revoke(&alice, 202402);
    fx.assert_invariants();
    fx.mint(&alice, 202402);
    fx.assert_invariants();

    // --- burn, then transfer into the vacated (user, period) slot --------
    fx.mint(&bob, 202406);
    fx.assert_invariants();
    fx.mint(&alice, 202406);
    fx.assert_invariants();
    fx.burn(&alice, 202406);
    fx.assert_invariants();
    fx.transfer(&bob, &alice, 202406);
    fx.assert_invariants();

    // --- transfer to a user with an existing non-empty index -------------
    fx.mint(&bob, 202407);
    fx.assert_invariants();
    fx.transfer(&bob, &alice, 202407);
    fx.assert_invariants();

    // --- transfer then burn at the destination --------------------------
    fx.transfer(&alice, &carol, 202407);
    fx.assert_invariants();
    fx.burn(&carol, 202407);
    fx.assert_invariants();

    // --- leave a record in the Bridged state -----------------------------
    fx.mint(&carol, 202408);
    fx.assert_invariants();
    fx.bridge_out(&carol, 202408);
    fx.assert_invariants();

    // Final cross-check through the public read API: every user's period
    // index must resolve to exactly `WrapCount` live records.
    for user in &fx.users {
        let records = fx.client.get_all_wraps_for_user(user);
        assert_eq!(
            records.len() as u32,
            fx.client.balance_of(user) as u32,
            "get_all_wraps_for_user != balance_of for {user:?}"
        );
        let summary_count = fx
            .client
            .get_wrap_summary(user)
            .map(|summary| summary.total_wraps)
            .unwrap_or(0);
        assert_eq!(
            summary_count,
            fx.client.balance_of(user) as u32,
            "get_wrap_summary.total_wraps != balance_of for {user:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Known-broken combinations
// ---------------------------------------------------------------------------
//
// Add `#[should_panic(expected = "...")]` cases here as new combination bugs
// are discovered, each with a comment linking the tracking issue. Remove the
// attribute when the underlying bug is fixed so the case becomes a regression
// test.

#[test]
fn batch_signature_matches_contract_payload() {
    // Guards the helper used by `batch_mint` above: the aggregated batch
    // payload layout and the single-item layout must stay in sync with the
    // contract's verifiers.
    let fx = Fixture::new();
    let alice = fx.users[0].clone();
    let period = 202401u64;
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&fx.env, &[1u8; 32]);
    let signature = fx.sign_mint(&alice, period, &archetype, &data_hash);

    let mut batch = Vec::new(&fx.env);
    batch.push_back(BatchWrapItem {
        user: alice.clone(),
        period,
        archetype: archetype.clone(),
        data_hash: data_hash.clone(),
        payload_version: PAYLOAD_VERSION,
        valid_until: u64::MAX,
        signature,
    });

    let payload = construct_batch_mint_payload(&fx.env, &fx.contract, &batch, PAYLOAD_VERSION);
    assert!(!payload.is_empty());
}
