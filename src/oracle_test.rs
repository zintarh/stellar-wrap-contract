use super::{StellarWrapContract, StellarWrapContractClient};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, BytesN, Env};

const APPROVED_HASH: [u8; 32] = [7; 32];

mod mock_oracle {
    use super::*;

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool {
            data_hash == BytesN::from_array(&e, &APPROVED_HASH)
        }
    }
}

mod failing_oracle {
    use super::*;

    #[contract]
    pub struct FailingOracle;

    #[contractimpl]
    impl FailingOracle {
        pub fn verify_data_hash(_e: Env, _data_hash: BytesN<32>) -> bool {
            panic!("oracle unavailable")
        }
    }
}

mod wrong_abi_oracle {
    use super::*;

    #[contract]
    pub struct WrongAbiOracle;

    #[contractimpl]
    impl WrongAbiOracle {
        pub fn verify_data_hash(_e: Env, _data_hash: BytesN<32>) -> u32 {
            1
        }
    }
}

mod missing_method_oracle {
    use super::*;

    #[contract]
    pub struct MissingMethodOracle;

    #[contractimpl]
    impl MissingMethodOracle {
        pub fn ping(_e: Env) -> bool {
            true
        }
    }
}

#[test]
fn oracle_accepts_recognized_hash() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let oracle_id = env.register(mock_oracle::MockOracle, ());
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let data_hash = BytesN::from_array(&env, &APPROVED_HASH);

    assert!(wrap.verify_with_oracle(&oracle_id, &data_hash));
}

#[test]
fn oracle_rejection_is_returned() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let oracle_id = env.register(mock_oracle::MockOracle, ());
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let unknown_hash = BytesN::from_array(&env, &[8; 32]);

    assert!(!wrap.verify_with_oracle(&oracle_id, &unknown_hash));
}

#[test]
fn oracle_failure_is_not_treated_as_rejection() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let oracle_id = env.register(failing_oracle::FailingOracle, ());
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let data_hash = BytesN::from_array(&env, &APPROVED_HASH);

    assert!(wrap.try_verify_with_oracle(&oracle_id, &data_hash).is_err());
}

#[test]
fn non_contract_oracle_address_fails() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let non_contract = Address::generate(&env);
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let data_hash = BytesN::from_array(&env, &APPROVED_HASH);

    assert!(wrap
        .try_verify_with_oracle(&non_contract, &data_hash)
        .is_err());
}

#[test]
fn incompatible_oracle_return_type_fails() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let oracle_id = env.register(wrong_abi_oracle::WrongAbiOracle, ());
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let data_hash = BytesN::from_array(&env, &APPROVED_HASH);

    assert!(wrap.try_verify_with_oracle(&oracle_id, &data_hash).is_err());
}

#[test]
fn missing_oracle_method_fails() {
    let env = Env::default();
    let wrap_id = env.register(StellarWrapContract, ());
    let oracle_id = env.register(missing_method_oracle::MissingMethodOracle, ());
    let wrap = StellarWrapContractClient::new(&env, &wrap_id);
    let data_hash = BytesN::from_array(&env, &APPROVED_HASH);

    assert!(wrap.try_verify_with_oracle(&oracle_id, &data_hash).is_err());
}
