#![no_main]

//! Fuzz harness for `mint_wrap`.
//!
//! Exercises signature verification, period validation, duplicate rejection,
//! and storage invariants under adversarial inputs. See README "Fuzzing".

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    symbol_short,
    testutils::arbitrary::{arbitrary, Arbitrary},
    testutils::Address as _,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, Symbol,
};
use stellar_wrap_contract::{StellarWrapContract, StellarWrapContractClient};

/// Structured fuzz input for `mint_wrap`.
#[derive(Clone, Debug, Arbitrary)]
pub struct MintWrapInput {
    /// Candidate period (`YYYYMM` when valid).
    pub period: u64,
    /// Raw hash bytes under test.
    pub data_hash: [u8; 32],
    /// Raw signature bytes (used when `use_valid_signature` is false).
    pub rogue_signature: [u8; 64],
    /// When true, sign the canonical payload with the admin key.
    pub use_valid_signature: bool,
    /// When true, invoke `mint_wrap` a second time with the same `(user, period)`.
    pub remint: bool,
}

fn sign_payload(
    env: &Env,
    signer: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
) -> BytesN<64> {
    let payload = stellar_wrap_contract::signature::construct_mint_payload(
        env, contract, user, period, archetype, data_hash, 1,
    );

    extern crate alloc;
    use alloc::vec::Vec;
    
    let len = payload.len() as usize;
    let mut out = Vec::with_capacity(len);
    out.resize(len, 0u8);
    payload.copy_into_slice(&mut out);
    let signature = signer.sign(&out);
    BytesN::from_array(env, &signature.to_bytes())
}

fn period_is_valid(period: u64) -> bool {
    let year = period / 100;
    let month = period % 100;
    (2024..=2100).contains(&year) && (1..=12).contains(&month)
}

fuzz_target!(|input: MintWrapInput| {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    // Fixed admin signing key so valid signatures are reproducible across runs.
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.initialize(&admin, &admin_pubkey);

    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&env, &input.data_hash);
    let signature = if input.use_valid_signature {
        sign_payload(
            &env,
            &signing_key,
            &contract_id,
            &user,
            input.period,
            &archetype,
            &data_hash,
        )
    } else {
        BytesN::from_array(&env, &input.rogue_signature)
    };

    let before = client.balance_of(&user);
    let result = client.try_mint_wrap(
        &user,
        &input.period,
        &archetype,
        &data_hash,
        &1u32,
        &signature,
    );
    let minted_ok = matches!(result, Ok(Ok(())));

    if minted_ok {
        assert!(
            period_is_valid(input.period),
            "mint succeeded with invalid period {}",
            input.period
        );
        assert!(
            input.use_valid_signature,
            "mint succeeded without a valid admin signature"
        );
        assert!(
            client.get_wrap(&user, &input.period).is_some(),
            "successful mint must persist a wrap record"
        );
        assert_eq!(
            client.balance_of(&user),
            before + 1,
            "successful mint must increment wrap count"
        );

        if input.remint {
            let remint = client.try_mint_wrap(
                &user,
                &input.period,
                &archetype,
                &data_hash,
                &1u32,
                &signature,
            );
            assert!(
                !matches!(remint, Ok(Ok(()))),
                "remint of the same (user, period) must fail"
            );
            assert_eq!(
                client.balance_of(&user),
                before + 1,
                "failed remint must not change balance"
            );
            assert!(
                client.get_wrap(&user, &input.period).is_some(),
                "original wrap must remain after failed remint"
            );
        }
    } else {
        // Rejected mints must not mutate storage for this user/period.
        assert!(
            client.get_wrap(&user, &input.period).is_none(),
            "rejected mint must not leave a wrap"
        );
        assert_eq!(
            client.balance_of(&user),
            before,
            "rejected mint must not change balance"
        );

        if input.use_valid_signature && period_is_valid(input.period) {
            // Valid period + admin signature should only fail for unexpected host issues.
            // Treat that as a fuzzer finding worth panicking on.
            panic!(
                "mint_wrap unexpectedly rejected valid period {} with admin signature: {result:?}",
                input.period
            );
        }
    }
});
