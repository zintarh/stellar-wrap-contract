#![no_std]
// `contractimpl` expands the contract entrypoint wrapper through the same host
// primitive that `contractclient` wraps. The handwritten edge below remains
// typed; the repository-wide rule still rejects direct calls in other code.
#![allow(clippy::disallowed_methods)]

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use wrap_types::WrapRegistryClient;

#[contract]
pub struct BatchProxy;

#[contractimpl]
impl BatchProxy {
    pub fn batch_mint_wrap(
        env: Env,
        wrap_contract: Address,
        recipients: Vec<Address>,
        amounts: Vec<i128>,
    ) {
        if recipients.len() != amounts.len() {
            panic!("recipients and amounts length mismatch");
        }
        for i in 0..recipients.len() {
            let recipient = recipients.get(i).unwrap();
            let amount = amounts.get(i).unwrap();
            WrapRegistryClient::new(&env, &wrap_contract).mint_wrap(&recipient, &amount);
        }
    }
}
