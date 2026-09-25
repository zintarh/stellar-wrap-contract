use soroban_sdk::{Address, Env, Symbol, Vec};

use crate::storage_types::MintWrapRequest;

pub fn emit_batch_mint_wrap(
    env: &Env,
    wrap_contract: &Address,
    requests: &Vec<MintWrapRequest>,
) {
    let topics = (Symbol.new(env, "batch_mint_wrap"), wrap_contract.clone());
    env.events().publish(topics, requests.clone());
}
