#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as TestAddress, Ledger},
    Address, BytesN, Env, Symbol,
};

#[test]
fn test_minting_flow() {
    let env = Env::default();
    
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    
    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);
    
    client.initialize(&admin);
    env.mock_all_auths();
    
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("soroban_architect");
    let period_id = 202401u64;
    
    client.mint_wrap(&user, &dummy_hash, &archetype, &period_id);
    
    let wrap_opt = client.get_wrap(&user, &period_id);
    
    assert!(wrap_opt.is_some());
    let wrap = wrap_opt.unwrap();

    assert_eq!(wrap.data_hash, dummy_hash);
    assert_eq!(wrap.archetype, archetype);
    assert_eq!(wrap.minted_at, env.ledger().timestamp());
    
    assert_eq!(client.get_user_count(&user), 1);
}

#[test]
#[should_panic(expected = "Error(ContractError(1))")]
fn test_initialize_twice_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    
    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
#[should_panic(expected = "Error(ContractError(3))")]
fn test_mint_wrap_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);
    let unauthorized = TestAddress::generate(&env);
    
    client.initialize(&admin);
    
    use soroban_sdk::symbol_short;
    let dummy_hash = BytesN::from_array(&env, &[42u8; 32]);
    let archetype = symbol_short!("defi_degenerate");
    let period_id = 202401u64;
    
    client.mint_wrap(&user, &dummy_hash, &archetype, &period_id);
}

#[test]
fn test_multiple_periods() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);
    
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype_1 = symbol_short!("soroban_architect");
    let archetype_2 = symbol_short!("defi_patron");
    let period_1 = 202401u64;
    let period_2 = 202402u64;
    
    client.mint_wrap(&user, &dummy_hash_1, &archetype_1, &period_1);
    client.mint_wrap(&user, &dummy_hash_2, &archetype_2, &period_2);
    
    let wrap_jan = client.get_wrap(&user, &period_1).unwrap();
    let wrap_feb = client.get_wrap(&user, &period_2).unwrap();
    
    assert_eq!(wrap_jan.data_hash, dummy_hash_1);
    assert_eq!(wrap_jan.archetype, archetype_1);
    
    assert_eq!(wrap_feb.data_hash, dummy_hash_2);
    assert_eq!(wrap_feb.archetype, archetype_2);
    
    assert_eq!(client.get_user_count(&user), 2);
}

#[test]
#[should_panic(expected = "Error(ContractError(4))")]
fn test_duplicate_period_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let admin = TestAddress::generate(&env);
    let user = TestAddress::generate(&env);
    
    client.initialize(&admin);
    env.mock_all_auths();

    use soroban_sdk::symbol_short;
    let dummy_hash_1 = BytesN::from_array(&env, &[42u8; 32]);
    let dummy_hash_2 = BytesN::from_array(&env, &[99u8; 32]);
    let archetype = symbol_short!("soroban_architect");
    let period_id = 202401u64;
    
    client.mint_wrap(&user, &dummy_hash_1, &archetype, &period_id);
    client.mint_wrap(&user, &dummy_hash_2, &archetype, &period_id);
}

#[test]
fn test_storage_types_xdr_validity() {
    let env = Env::default();
    
    use soroban_sdk::symbol_short;
    use storage_types::WrapRecord;
    
    let minted_at = 1234567890u64;
    let data_hash = BytesN::from_array(&env, &[1u8; 32]);
    let archetype = symbol_short!("soroban_dev");
    
    let wrap_record = WrapRecord {
        minted_at,
        data_hash: data_hash.clone(),
        archetype: archetype.clone(),
    };
    
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);
    
    let admin = TestAddress::generate(&env);
    client.initialize(&admin);
    env.mock_all_auths();
    
    let period_id = 1u64;
    client.mint_wrap(&admin, &data_hash, &archetype, &period_id);
    let retrieved = client.get_wrap(&admin, &period_id).unwrap();
    
    assert_eq!(retrieved.minted_at, minted_at);
    assert_eq!(retrieved.data_hash, data_hash);
    assert_eq!(retrieved.archetype, archetype);
    
    use storage_types::DataKey;
    
    let admin_key = DataKey::Admin;
    let wrap_key = DataKey::Wrap(admin.clone(), period_id);
    let user_count_key = DataKey::UserCount(admin.clone());
    
    let count = client.get_user_count(&admin);
    assert_eq!(count, 1);
}
