//! # Staking mechanism for Wrap priority
//!
//! Users can stake tokens to earn fee discounts (priority) when minting wraps.
//! The discount is calculated based on the amount staked relative to the
//! admin-configured `min_stake`. A cooldown period applies before unstaked
//! funds can be withdrawn.
//!
//! ## Invariant
//!
//! `total_staked` (stored under [`DataKey::TotalStaked`]) equals the sum of the
//! `amount` fields of all live [`StakeRecord`] entries at every observable
//! point. Every write path that mutates a stake record must update the
//! aggregate in the same call; [`assert_total_staked_invariant`] verifies this
//! and is exercised by the property tests and exposed as an operator entrypoint.
//!
//! ## Storage access
//!
//! All persistent and instance storage access for stake state goes through the
//! `store` module (see `crate::store::stake`). This module never calls
//! `e.storage()` directly.

use soroban_sdk::{panic_with_error, symbol_short, Address, Env};

use crate::store::stake as stake_store;
use crate::{
    storage_types::{StakeConfig, StakeRecord},
    ContractError,
};

const DEFAULT_MIN_STAKE: i128 = 100;
const DEFAULT_COOLDOWN_SECONDS: u64 = 7 * 24 * 60 * 60; // 7 days
const DEFAULT_PRIORITY_MULTIPLIER_BPS: u32 = 1_000; // 10% per min_stake unit above minimum
const DEFAULT_MAX_PRIORITY_BPS: u32 = 5_000; // 50% max discount

// ── Config helpers ──────────────────────────────────────────────────────────

/// Read the current stake config, falling back to sensible defaults when none
/// has been set.
fn read_stake_config(e: &Env) -> StakeConfig {
    stake_store::config(e).unwrap_or(StakeConfig {
        min_stake: DEFAULT_MIN_STAKE,
        cooldown_seconds: DEFAULT_COOLDOWN_SECONDS,
        priority_multiplier_bps: DEFAULT_PRIORITY_MULTIPLIER_BPS,
        max_priority_bps: DEFAULT_MAX_PRIORITY_BPS,
    })
}

// ── Invariant ───────────────────────────────────────────────────────────────

/// Assert the accounting invariant: `total_staked` equals the sum of all live
/// stake records. Panics with [`ContractError::StakeAccountingMismatch`] when
/// the aggregate has drifted from its constituents.
///
/// This is the single source of truth for the invariant and is called after
/// every mutating stake operation as well as exposed as an operator entrypoint.
pub(crate) fn assert_total_staked_invariant(e: &Env) {
    let recorded = stake_store::total_staked(e);
    let summed = stake_store::sum_live_stakes(e);
    if recorded != summed {
        panic_with_error!(e, ContractError::StakeAccountingMismatch);
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Stake tokens for `user`.
///
/// # Authorization
/// `user` must authorize the call (`user.require_auth()`).
///
/// # Panics
/// - [`ContractError::StakeTooLow`] if `amount < config.min_stake`.
/// - [`ContractError::StakeCooldownActive`] if an unstake is already in progress.
/// - [`ContractError::StakeArithmeticOverflow`] on total overflow.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn stake(e: Env, user: Address, amount: i128) {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    let config = read_stake_config(&e);

    if amount < config.min_stake {
        panic_with_error!(e, ContractError::StakeTooLow);
    }

    if let Some(existing) = stake_store::record(&e, &user) {
        // If an unstake is already in progress, reject new stakes until resolved.
        if existing.unstaking_at != 0 {
            panic_with_error!(e, ContractError::StakeCooldownActive);
        }

        let new_amount = existing
            .amount
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::StakeArithmeticOverflow));

        let updated = StakeRecord {
            amount: new_amount,
            staked_at: existing.staked_at, // keep original stake time
            unstaking_at: 0,
        };
        stake_store::set_record(&e, &user, &updated);

        let total = stake_store::total_staked(&e);
        let new_total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::StakeArithmeticOverflow));
        stake_store::set_total_staked(&e, new_total);

        e.events().publish(
            (symbol_short!("stake"), user.clone(), symbol_short!("add")),
            amount,
        );
    } else {
        let now = e.ledger().timestamp();
        let record = StakeRecord {
            amount,
            staked_at: now,
            unstaking_at: 0,
        };
        stake_store::set_record(&e, &user, &record);

        let total = stake_store::total_staked(&e);
        let new_total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::StakeArithmeticOverflow));
        stake_store::set_total_staked(&e, new_total);

        e.events().publish(
            (symbol_short!("stake"), user.clone(), symbol_short!("init")),
            amount,
        );
    }

    assert_total_staked_invariant(&e);
}

/// Initiate the unstaking process for `user`.
///
/// This starts the cooldown timer. After `cooldown_seconds`, the user can
/// call [`withdraw_stake`] to retrieve their staked amount.
///
/// # Authorization
/// `user` must authorize the call.
///
/// # Panics
/// - [`ContractError::StakeNotFound`] if the user has no active stake.
/// - [`ContractError::StakeCooldownActive`] if an unstake is already in progress.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn unstake(e: Env, user: Address) {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    let mut record = stake_store::record(&e, &user)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::StakeNotFound));

    if record.unstaking_at != 0 {
        panic_with_error!(e, ContractError::StakeCooldownActive);
    }

    let now = e.ledger().timestamp();
    record.unstaking_at = now;
    stake_store::set_record(&e, &user, &record);

    e.events()
        .publish((symbol_short!("unstake"), user), record.amount);

    assert_total_staked_invariant(&e);
}

/// Complete the unstaking process and withdraw staked funds.
///
/// Can only be called after the cooldown period has elapsed since
/// [`unstake`] was called.
///
/// # Authorization
/// `user` must authorize the call.
///
/// # Panics
/// - [`ContractError::StakeNotFound`] if the user has no stake record.
/// - [`ContractError::StakeNotUnstaking`] if unstake was never initiated.
/// - [`ContractError::StakeCooldownNotElapsed`] if cooldown hasn't passed.
#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn withdraw_stake(e: Env, user: Address) {
    crate::admin::require_not_paused(&e);
    user.require_auth();

    let record = stake_store::record(&e, &user)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::StakeNotFound));

    if record.unstaking_at == 0 {
        panic_with_error!(e, ContractError::StakeNotUnstaking);
    }

    let config = read_stake_config(&e);
    let now = e.ledger().timestamp();

    if now.saturating_sub(record.unstaking_at) < config.cooldown_seconds {
        panic_with_error!(e, ContractError::StakeCooldownNotElapsed);
    }

    let withdrawn = record.amount;

    let total = stake_store::total_staked(&e);
    let new_total = total.saturating_sub(withdrawn);
    stake_store::set_total_staked(&e, new_total);

    stake_store::remove_record(&e, &user);

    e.events()
        .publish((symbol_short!("withdraw"), user), withdrawn);

    assert_total_staked_invariant(&e);
}

// ── Queries ─────────────────────────────────────────────────────────────────

/// Return the stake record for `user`, or `None` if they have not staked.
pub(crate) fn get_stake(e: &Env, user: Address) -> Option<StakeRecord> {
    stake_store::record(e, &user)
}

/// Compute the fee-discount priority for `user` in basis points.
///
/// The priority is calculated as:
/// `min( (amount / min_stake) * priority_multiplier_bps, max_priority_bps )`
///
/// Returns 0 if the user has no stake or is in the unstaking process.
pub(crate) fn get_stake_priority(e: &Env, user: Address) -> u32 {
    let record = match stake_store::record(e, &user) {
        Some(r) => r,
        None => return 0,
    };

    // No priority during an active unstake.
    if record.unstaking_at != 0 {
        return 0;
    }

    let config = read_stake_config(e);
    if config.min_stake == 0 || record.amount < config.min_stake {
        return 0;
    }

    // Stay in i128 until the final cast to avoid intermediate overflow.
    let units = record.amount / config.min_stake;
    let bps = units.saturating_mul(config.priority_multiplier_bps as i128);
    let capped = if bps > config.max_priority_bps as i128 {
        config.max_priority_bps as i128
    } else {
        bps
    };
    capped as u32
}

/// Operator entrypoint: verify the `total_staked` accounting invariant.
///
/// Panics with [`ContractError::StakeAccountingMismatch`] if the aggregate has
/// drifted from the sum of live stake records. Consistent with the
/// storage-invariant self-check introduced in issue #728.
pub(crate) fn check_total_staked_invariant(e: &Env) {
    assert_total_staked_invariant(e);
}
