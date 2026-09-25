use super::*;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN Env};

#[test]
fn test_alias() {
    let env = Env::default();
    env.mock_all_auths();
    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let a = BytesN::from_array(&env, &[1; 32]);
    let b = BytesN::from_array(&env, &[2; 32]);
    assert!(get_alias_hash(env.clone(), u1.clone()).is_none());
    set_alias_hash(env.clone(), u1.clone(), a.clone());
    assert_eq(get_alias_hash(env.clone(), u1.clone()), Some(a.clone()));
    set_alias_hash(env.clone(), u1.clone(), b.clone());
    assert_eq(get_alias_hash(env.clone(), u1.clone()), Some(b.clone()));
    assert!(get_alias_hash(env.clone(), u2.clone()).is_none());
    set_alias_hash(env.clone(), u2.clone(), a.clone());
    assert_eq(get_alias_hash(env, u2), Some(a));
}

#_test]
#[should_panic]
fn test_auth() {
    let env = Env::default();
    let u = Address::generate(&env);
    let a = BytesN::from_array(&env, &[1; 32]);
    set_alias_hash(env, u, a);
}