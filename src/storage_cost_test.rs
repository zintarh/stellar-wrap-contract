//! Storage-rent benchmark for a three-year user (#1033).
//!
//! The contract's shipped layout (v1) and the proposed `UserIndex` layout (v2)
//! are modelled by the probe contract below and driven through the same
//! cadence: 36 monthly writes plus a yearly renewal of every existing entry.
//!
//! Rent is read from `Env::cost_estimate().fee().persistent_entry_rent` after
//! every invocation, so the comparison uses the SDK's mainnet fee snapshot
//! rather than hand-computed constants. Run:
//!
//! ```text
//! cargo test storage_cost -- --nocapture
//! ```
//!
//! The table printed by the test is mirrored in `docs/storage-costs.md`.

extern crate std;

use std::vec::Vec as StdVec;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

const MONTHS: u32 = 36;
const YEAR_MONTHS: u32 = 12;
const MONTH_SECONDS: u64 = 30 * 24 * 60 * 60;
const MONTH_LEDGERS: u32 = 30 * 24 * 60 * 12; // 5-second ledgers
const TTL_ONE_YEAR: u32 = 17_280 * 365;

fn advance_month(env: &Env) {
    env.ledger().with_mut(|ledger| {
        ledger.timestamp += MONTH_SECONDS;
        ledger.sequence_number += MONTH_LEDGERS;
    });
}

/// Proposed per-user index: replaces `WrapCount`, `LatestPeriod`,
/// `UserPeriods` and `WrapPeriods` with one persistent entry.
#[contracttype]
#[derive(Clone)]
pub struct UserIndex {
    pub count: u32,
    pub latest_period: u64,
    pub periods: Vec<u64>,
}

#[contracttype]
#[derive(Clone)]
pub enum LayoutKey {
    Wrap(Address, u64),
    WrapCount(Address),
    LatestPeriod(Address),
    UserPeriods(Address),
    WrapPeriods(Address),
    TotalWrapCount,
    LastUpdated(Address),
    UserIndex(Address),
}

#[contract]
pub struct LayoutProbe;

#[contractimpl]
impl LayoutProbe {
    /// v1-shaped monthly write: record + count + latest + two period indexes.
    pub fn v1_month(env: Env, user: Address, period: u64, payload: BytesN<32>) {
        let wrap_key = LayoutKey::Wrap(user.clone(), period);
        env.storage().persistent().set(&wrap_key, &payload);
        env.storage()
            .persistent()
            .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        let count_key = LayoutKey::WrapCount(user.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage().persistent().set(&count_key, &(count + 1));
        env.storage()
            .persistent()
            .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        let latest_key = LayoutKey::LatestPeriod(user.clone());
        env.storage().persistent().set(&latest_key, &period);
        env.storage()
            .persistent()
            .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        for key in [
            LayoutKey::UserPeriods(user.clone()),
            LayoutKey::WrapPeriods(user.clone()),
        ] {
            let mut periods: Vec<u64> = env.storage().persistent().get(&key).unwrap_or_default();
            periods.push_back(period);
            env.storage().persistent().set(&key, &periods);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }

        Self::write_shared(&env, user);
    }

    /// v2-shaped monthly write: record + one combined `UserIndex` entry.
    pub fn v2_month(env: Env, user: Address, period: u64, payload: BytesN<32>) {
        let wrap_key = LayoutKey::Wrap(user.clone(), period);
        env.storage().persistent().set(&wrap_key, &payload);
        env.storage()
            .persistent()
            .extend_ttl(&wrap_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        let index_key = LayoutKey::UserIndex(user.clone());
        let mut index: UserIndex =
            env.storage()
                .persistent()
                .get(&index_key)
                .unwrap_or(UserIndex {
                    count: 0,
                    latest_period: 0,
                    periods: Vec::new(&env),
                });
        index.count += 1;
        index.periods.push_back(period);
        if period > index.latest_period {
            index.latest_period = period;
        }
        env.storage().persistent().set(&index_key, &index);
        env.storage()
            .persistent()
            .extend_ttl(&index_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        Self::write_shared(&env, user);
    }

    /// v1 yearly renewal of every entry written above.
    pub fn v1_renew(env: Env, user: Address, periods: Vec<u64>) {
        for period in periods.iter() {
            let key = LayoutKey::Wrap(user.clone(), period);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
        for key in [
            LayoutKey::WrapCount(user.clone()),
            LayoutKey::LatestPeriod(user.clone()),
            LayoutKey::UserPeriods(user.clone()),
            LayoutKey::WrapPeriods(user.clone()),
            LayoutKey::LastUpdated(user),
            LayoutKey::TotalWrapCount,
        ] {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    }

    /// v2 yearly renewal: records + the single `UserIndex` entry.
    pub fn v2_renew(env: Env, user: Address, periods: Vec<u64>) {
        for period in periods.iter() {
            let key = LayoutKey::Wrap(user.clone(), period);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
        for key in [
            LayoutKey::UserIndex(user.clone()),
            LayoutKey::LastUpdated(user),
            LayoutKey::TotalWrapCount,
        ] {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_ONE_YEAR, TTL_ONE_YEAR);
        }
    }
}

impl LayoutProbe {
    /// Entries written on every mint in both layouts: the global counter and
    /// the user's last-updated timestamp.
    fn write_shared(env: &Env, user: Address) {
        let total_key = LayoutKey::TotalWrapCount;
        let total: u32 = env.storage().persistent().get(&total_key).unwrap_or(0);
        env.storage().persistent().set(&total_key, &(total + 1));
        env.storage()
            .persistent()
            .extend_ttl(&total_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

        let updated_key = LayoutKey::LastUpdated(user);
        env.storage()
            .persistent()
            .set(&updated_key, &env.ledger().timestamp());
        env.storage()
            .persistent()
            .extend_ttl(&updated_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    }
}

fn measure(v2: bool) -> i64 {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LayoutProbe, ());
    let client = LayoutProbeClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let payload = BytesN::from_array(&env, &[42u8; 32]);
    let mut periods: StdVec<u64> = StdVec::new();
    let mut rent: i64 = 0;

    for index in 0..MONTHS {
        advance_month(&env);
        let period = 202_601 + (index / 12) as u64 * 100 + (index % 12) as u64;
        if v2 {
            client.v2_month(&user, &period, &payload);
        } else {
            client.v1_month(&user, &period, &payload);
        }
        rent += env.cost_estimate().fee().persistent_entry_rent;
        periods.push(period);

        if (index + 1) % YEAR_MONTHS == 0 {
            let batch = Vec::from_slice(&env, &periods);
            if v2 {
                client.v2_renew(&user, &batch);
            } else {
                client.v1_renew(&user, &batch);
            }
            rent += env.cost_estimate().fee().persistent_entry_rent;
        }
    }

    rent
}

fn xlm(stroops: i64) -> f64 {
    stroops as f64 / 10_000_000.0
}

#[test]
fn benchmark_three_year_storage_rent() {
    let v1 = measure(false);
    let v2 = measure(true);
    let savings = 100.0 * (v1 - v2) as f64 / v1 as f64;

    std::println!("\n### Three-year storage rent (persistent_entry_rent, stroops)\n");
    std::println!("| Layout | 36 monthly writes + 3 renewals | XLM |");
    std::println!("| --- | ---: | ---: |");
    std::println!("| v1 (shipped) | {v1} | {:.4} |", xlm(v1));
    std::println!("| v2 (`UserIndex`) | {v2} | {:.4} |", xlm(v2));
    std::println!("\nSavings with `UserIndex`: {savings:.1}%\n");

    assert!(v1 > 0, "v1 must produce rent");
    assert!(v2 > 0, "v2 must produce rent");
    assert!(v2 < v1, "UserIndex must cost less (v1={v1}, v2={v2})");
}
