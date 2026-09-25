extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Address, Env, Symbol, Vec};

use crate::{BatchProxy, BatchProxyClient};

mod mock_wrap {
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

    #[contract]
    pub struct MockWrap;

    #[contractimpl]
    impl MockWrap {
        pub fn mint_wrap(env: Env, recipient: Address, amount: i128) {
            if env
                .storage()
                .instance()
                .get::<i128>(&Symbol::new(&env, "fail"))
                == Some(amount)
            {
                panic!("forced failure");
            }
            let key = Symbol::new(&env, "minted");
            let mut minted: Vec<(Address, i128)> = env
                .storage()
                .instance()
                .get(&key)
                .unwrap_or(Vec::new(&env));
            minted.push_back((recipient, amount));
            env.storage().instance().set(&key, &minted);
        }

        pub fn set_fail_amount(env: Env, amount: i128) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "fail"), &amount);
        }

        pub fn minted(env: Env) -> Vec<(Address, i128)> {
            let empty: Vec<(Address, i128)> = Vec::new(&env);
            env.storage()
                .instance()
                .get(&Symbol::new(&env, "minted"))
                .unwrap_or(empty)
        }
    }
}

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let proxy_id = env.register_contract(None, BatchProxy);
    let mock_id = env.register_contract(None, mock_wrap::MockWrap);
    (env, proxy_id, mock_id)
}

fn create_recipients(env: &Env, count: u32) -> (Vec<Address>, Vec<i128>) {
    let mut recipients = Vec::new(env);
    let mut amounts = Vec::new(env);
    for i in 0..count {
        recipients.push_back(Address::generate(env));
        amounts.push_back((i + 1) as i128);
    }
    (recipients, amounts)
}

#[test]
fn test_batch_mint_wrap_succeeds() {
    let (env, proxy_id, mock_id) = setup();
    let (recipients, amounts) = create_recipients(&env, 2);

    let proxy_client = BatchProxyClient::new(&env, &proxy_id);
    proxy_client.batch_mint_wrap(&mock_id, &recipients, &amounts);

    let mock_client = mock_wrap::MockWrapClient::new(&env, &mock_id);
    let minted = mock_client.minted();
    assert_eq!(minted.len(), 2);
    assert_eq!(minted.get(0).unwrap().0, recipients.get(0).unwrap());
    assert_eq!(minted.get(0).unwrap().1, amounts.get(0).unwrap());
    assert_eq!(minted.get(1).unwrap().0, recipients.get(1).unwrap());
    assert_eq!(minted.get(1).unwrap().1, amounts.get(1).unwrap());
}

#[test]
fn test_batch_mint_wrap_empty_succeeds() {
    let (env, proxy_id, mock_id) = setup();
    let recipients: Vec<Address> = Vec::new(&env);
    let amounts: Vec<i128> = Vec::new(&env);

    let proxy_client = BatchProxyClient::new(&env, &proxy_id);
    proxy_client.batch_mint_wrap(&mock_id, &recipients, &amounts);

    let mock_client = mock_wrap::MockWrapClient::new(&env, &mock_id);
    assert_eq!(mock_client.minted().len(), 0);
}

#[test]
#[should_panic(expected = "recipients and amounts length mismatch")]
fn test_batch_mint_wrap_length_mismatch_panics() {
    let (env, proxy_id, mock_id) = setup();
    let (recipients, _) = create_recipients(&env, 2);
    let amounts = vec![&env, 10_i128];

    let proxy_client = BatchProxyClient::new(&env, &proxy_id);
    proxy_client.batch_mint_wrap(&mock_id, &recipients, &amounts);
}

#[test]
fn test_batch_mint_wrap_atomic_rollback_on_failure() {
    let (env, proxy_id, mock_id) = setup();
    let (recipients, amounts) = create_recipients(&env, 2);

    let mock_client = mock_wrap::MockWrapClient::new(&env, &mock_id);
    let fail_amount = amounts.get(1).unwrap();
    mock_client.set_fail_amount(&fail_amount);

    let proxy_client = BatchProxyClient::new(&env, &proxy_id);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        proxy_client.batch_mint_wrap(&mock_id, &recipients, &amounts);
    }));

    assert!(result.is_err());
    assert_eq!(mock_client.minted().len(), 0);
}
