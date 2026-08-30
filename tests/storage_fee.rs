#![allow(deprecated)]

use soroban_sdk::Env;
use stellar_wrap_contract::{StellarWrapContract, StellarWrapContractClient};

#[test]
fn storage_accounting_compile_test() {
    let e = Env::default();
    let contract_id = e.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&e, &contract_id);
    let _ = client.storage_bytes();
}
