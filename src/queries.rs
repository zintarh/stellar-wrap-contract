use crate::{ContractHealth, DataKey, WrapRecord};
use soroban_sdk::{Address, Bytes, BytesN, Env, String};

pub(crate) fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
    e.storage().persistent().get(&DataKey::Wrap(user, period))
}

pub(crate) fn balance_of(e: Env, user: Address) -> i128 {
    let count_key = DataKey::WrapCount(user);
    e.storage()
        .persistent()
        .get::<_, u32>(&count_key)
        .unwrap_or(0) as i128
}

pub(crate) fn verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool {
    let wrap: Option<WrapRecord> = e.storage().persistent().get(&DataKey::Wrap(user, period));
    match wrap {
        Some(record) => {
            let computed_hash = e.crypto().sha256(&data);
            let computed_hash = BytesN::from_array(&e, &computed_hash.to_array());
            record.data_hash == computed_hash
        }
        None => false,
    }
}

pub(crate) fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
    let latest_key = DataKey::LatestPeriod(user.clone());
    let period: u64 = e.storage().persistent().get(&latest_key)?;
    e.storage().persistent().get(&DataKey::Wrap(user, period))
}

pub(crate) fn health(e: Env) -> ContractHealth {
    let has_admin = e.storage().instance().has(&DataKey::Admin);
    let has_signing_key = e.storage().instance().has(&DataKey::AdminPubKey);

    ContractHealth {
        initialized: has_admin,
        has_admin,
        has_signing_key,
    }
}

pub(crate) fn get_admin(e: Env) -> Option<Address> {
    e.storage().instance().get(&DataKey::Admin)
}

pub(crate) fn name(e: Env) -> String {
    String::from_str(&e, "Stellar Wrap Registry")
}

pub(crate) fn symbol(e: Env) -> String {
    String::from_str(&e, "WRAP")
}

pub(crate) fn decimals(_e: Env) -> u32 {
    0
}
