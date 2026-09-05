#`!no_std]

#c[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Vec};

#_contract]
pub struct BatchProxy;

#_contractimpl]
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
            env.invoke_contract::<()>(
                &wrap_contract,
                &symbol_short!("mint_wrap"),
                (recipient, amount),
            );
        }
    }
}
