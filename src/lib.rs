#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Symbol};

mod admin;
mod errors;
mod mint;
mod queries;
mod storage_types;

pub use errors::ContractError;
pub use storage_types::{DataKey, WrapRecord};

#[contract]
pub struct StellarWrapContract;

#[contractimpl]
impl StellarWrapContract {
    pub fn initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>) {
        admin::initialize(e, admin, admin_pubkey);
    }

    pub fn update_admin(e: Env, new_admin: Address) {
        admin::update_admin(e, new_admin);
    }

    pub fn mint_wrap(
        e: Env,
        user: Address,
        period: u64,
        archetype: Symbol,
        data_hash: BytesN<32>,
        signature: BytesN<64>,
    ) {
        mint::mint_wrap(e, user, period, archetype, data_hash, signature);
    }

    pub fn get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord> {
        queries::get_wrap(e, user, period)
    }

    pub fn balance_of(e: Env, user: Address) -> i128 {
        queries::balance_of(e, user)
    }

    pub fn verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool {
        queries::verify_data(e, user, period, data)
    }

    pub fn get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord> {
        queries::get_latest_wrap(e, user)
    }

    pub fn get_admin(e: Env) -> Option<Address> {
        queries::get_admin(e)
    }

    pub fn name(e: Env) -> String {
        queries::name(e)
    }

    pub fn symbol(e: Env) -> String {
        queries::symbol(e)
    }

    pub fn decimals(e: Env) -> u32 {
        queries::decimals(e)
    }
}

#[cfg(test)]
mod security_test;
#[cfg(test)]
mod test;
