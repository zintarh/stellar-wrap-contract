use soroban_sdk::{panic_with_error, symbol_short, token, Address, Env, Vec};

use crate::{admin, ContractError, DataKey, TransferFeeConfig, WrapRecord};

const TTL_ONE_YEAR: u32 = 17_280 * 365;

fn read_fee(e: &Env) -> Option<TransferFeeConfig> {
    e.storage().instance().get(&DataKey::TransferFee)
}

fn read_periods(e: &Env, owner: &Address, expected_count: u32) -> Vec<u64> {
    let periods: Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::WrapPeriods(owner.clone()))
        .unwrap_or_else(|| Vec::new(e));

    if periods.len() != expected_count {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }
    periods
}

pub(crate) fn contains_period(periods: &Vec<u64>, period: u64) -> bool {
    for stored_period in periods.iter() {
        if stored_period == period {
            return true;
        }
    }
    false
}

pub(crate) fn remove_period(e: &Env, periods: &Vec<u64>, period: u64) -> Vec<u64> {
    let mut remaining = Vec::new(e);
    for stored_period in periods.iter() {
        if stored_period != period {
            remaining.push_back(stored_period);
        }
    }
    remaining
}

pub(crate) fn latest_period(periods: &Vec<u64>) -> Option<u64> {
    let mut latest = None;
    for period in periods.iter() {
        if latest.map(|current| period > current).unwrap_or(true) {
            latest = Some(period);
        }
    }
    latest
}

pub(crate) fn write_owner_state(e: &Env, owner: &Address, periods: &Vec<u64>) {
    let count_key = DataKey::WrapCount(owner.clone());
    let latest_key = DataKey::LatestPeriod(owner.clone());
    let periods_key = DataKey::WrapPeriods(owner.clone());

    if periods.is_empty() {
        e.storage().persistent().remove(&count_key);
        e.storage().persistent().remove(&latest_key);
        e.storage().persistent().remove(&periods_key);
        return;
    }

    let count = periods.len();
    e.storage().persistent().set(&count_key, &count);
    e.storage().persistent().set(&periods_key, periods);
    e.storage()
        .persistent()
        .extend_ttl(&count_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
    e.storage()
        .persistent()
        .extend_ttl(&periods_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    let latest = latest_period(periods)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::StorageInvariantViolation));
    e.storage().persistent().set(&latest_key, &latest);
    e.storage()
        .persistent()
        .extend_ttl(&latest_key, TTL_ONE_YEAR, TTL_ONE_YEAR);
}

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn backfill_wrap_periods(e: Env, user: Address, periods: Vec<u64>) {
    admin::read_admin(&e).require_auth();

    let periods_key = DataKey::WrapPeriods(user.clone());
    if e.storage().persistent().has(&periods_key) {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }

    let expected_count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::WrapCount(user.clone()))
        .unwrap_or(0);
    if periods.len() != expected_count {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }

    for index in 0..periods.len() {
        let period = periods.get(index).unwrap();
        if !e
            .storage()
            .persistent()
            .has(&DataKey::Wrap(user.clone(), period))
        {
            panic_with_error!(e, ContractError::StorageInvariantViolation);
        }
        for previous in 0..index {
            if periods.get(previous).unwrap() == period {
                panic_with_error!(e, ContractError::StorageInvariantViolation);
            }
        }
    }

    write_owner_state(&e, &user, &periods);
    e.events()
        .publish((symbol_short!("backfill"), user), periods.len());
}

#[allow(deprecated)] // TODO(#718): migrate to #[contractevent]
pub(crate) fn transfer_wrap(e: Env, from: Address, to: Address, period: u64) {
    from.require_auth();

    if from == to {
        panic_with_error!(e, ContractError::InvalidTransfer);
    }

    let source_key = DataKey::Wrap(from.clone(), period);
    let destination_key = DataKey::Wrap(to.clone(), period);
    let record: WrapRecord = e
        .storage()
        .persistent()
        .get(&source_key)
        .unwrap_or_else(|| panic_with_error!(e, ContractError::WrapNotFound));

    if record.fsm.state == crate::storage_types::WrapState::Bridged {
        panic_with_error!(e, ContractError::InvalidStateTransition);
    }

    let fee = read_fee(&e);

    if e.storage().persistent().has(&destination_key) {
        panic_with_error!(e, ContractError::WrapAlreadyExists);
    }
    if e.storage().temporary().has(&DataKey::TransferGuard) {
        panic_with_error!(e, ContractError::TransferInProgress);
    }

    let source_count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::WrapCount(from.clone()))
        .unwrap_or(0);
    let source_periods = read_periods(&e, &from, source_count);
    if !contains_period(&source_periods, period) {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }

    let destination_count: u32 = e
        .storage()
        .persistent()
        .get(&DataKey::WrapCount(to.clone()))
        .unwrap_or(0);
    let mut destination_periods = read_periods(&e, &to, destination_count);
    if contains_period(&destination_periods, period) {
        panic_with_error!(e, ContractError::StorageInvariantViolation);
    }

    e.storage().temporary().set(&DataKey::TransferGuard, &true);

    if let Some(ref fee) = fee {
        if fee.amount > 0 {
            token::Client::new(&e, &fee.token).transfer(&from, &fee.recipient, &fee.amount);
        }
    }

    e.storage().persistent().remove(&source_key);
    e.storage().persistent().set(&destination_key, &record);
    e.storage()
        .persistent()
        .extend_ttl(&destination_key, TTL_ONE_YEAR, TTL_ONE_YEAR);

    let source_periods = remove_period(&e, &source_periods, period);
    destination_periods.push_back(period);
    write_owner_state(&e, &from, &source_periods);
    write_owner_state(&e, &to, &destination_periods);

    e.storage().temporary().remove(&DataKey::TransferGuard);
    if let Some(ref fee) = fee {
        e.events().publish(
            (symbol_short!("transfer"), from, to, period),
            (fee.token.clone(), fee.recipient.clone(), fee.amount),
        );
    } else {
        e.events().publish(
            (symbol_short!("transfer"), from, to, period),
            (),
        );
    }
}
