#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};

use super::{StellarWrapContract, StellarWrapContractClient};

#[test]
fn test_balance_of_starts_at_zero() {
    let env = Env::default();
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    assert_eq!(client.balance_of(&user), 0);
}
