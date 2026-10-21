#![no_std]
// The SDK expands `#[contractclient]` into the low-level invocation it wraps.
#![allow(clippy::disallowed_methods)]

use soroban_sdk::{contractclient, Address, BytesN, Env};

/// ABI shared by contracts that forward wrap-mint requests.
#[contractclient(name = "WrapRegistryClient")]
pub trait WrapRegistryApi {
    fn mint_wrap(e: Env, recipient: Address, amount: i128);
}

/// ABI implemented by a compatible data-hash oracle.
#[contractclient(name = "DataHashOracleClient")]
pub trait DataHashOracleApi {
    fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool;
}
