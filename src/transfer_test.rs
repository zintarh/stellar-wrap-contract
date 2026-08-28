#![cfg(test)]

extern crate std;

use super::*;
use crate::test_utils::sign_payload;
use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, BytesN, Env, IntoVal, Symbol, TryIntoVal,
};

struct Fixture {
    contract_id: Address,
    env: Env,
    fee_recipient: Address,
    from: Address,
    signing_key: SigningKey,
    to: Address,
    token_id: Address,
}

fn fixture(fee_amount: Option<i128>, sender_token_balance: i128) -> Fixture {
    let env = Env::default();
    let contract_id = env.register_contract(None, StellarWrapContract);
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&[21u8; 32]);
    let admin_pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);
    if let Some(amount) = fee_amount {
        client.set_transfer_fee(&token_id, &fee_recipient, &amount);
    }
    if sender_token_balance > 0 {
        StellarAssetClient::new(&env, &token_id).mint(&from, &sender_token_balance);
    }

    Fixture {
        contract_id,
        env,
        fee_recipient,
        from,
        signing_key,
        to,
        token_id,
    }
}

fn mint(fixture: &Fixture, owner: &Address, period: u64, hash_byte: u8) {
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&fixture.env, &[hash_byte; 32]);
    let signature = sign_payload(
        &fixture.env,
        &fixture.signing_key,
        &fixture.contract_id,
        owner,
        period,
        &archetype,
        &data_hash,
    );
    client.mint_wrap(owner, &period, &archetype, &data_hash, &1u32, &signature);
}

#[test]
fn transfer_moves_record_collects_fee_and_updates_indexes() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    let token = TokenClient::new(&fixture.env, &fixture.token_id);

    mint(&fixture, &fixture.from, 202401, 1);
    mint(&fixture, &fixture.from, 202403, 3);
    mint(&fixture, &fixture.from, 202402, 2);
    mint(&fixture, &fixture.to, 202405, 5);
    let original = client.get_wrap(&fixture.from, &202403).unwrap();

    client.transfer_wrap(&fixture.from, &fixture.to, &202403);

    // Read events immediately after the generating call; in SDK 27 a later
    // contract invocation clears the previously recorded event buffer.
    let events = crate::test_utils::decode_events(&fixture.env);
    let (topics, data) = events.last().expect("transfer event was not emitted");
    let event_name: Symbol = topics[0].try_into_val(&fixture.env).unwrap();
    let event_from: Address = topics[1].try_into_val(&fixture.env).unwrap();
    let event_to: Address = topics[2].try_into_val(&fixture.env).unwrap();
    let event_period: u64 = topics[3].try_into_val(&fixture.env).unwrap();
    let event_fee: (Address, Address, i128) = data.try_into_val(&fixture.env).unwrap();

    assert_eq!(event_name, symbol_short!("transfer"));
    assert_eq!(event_from, fixture.from);
    assert_eq!(event_to, fixture.to);
    assert_eq!(event_period, 202403);
    assert_eq!(
        event_fee,
        (fixture.token_id.clone(), fixture.fee_recipient.clone(), 10)
    );

    assert_eq!(
        fixture.env.auths(),
        std::vec![(
            fixture.from.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.contract_id.clone(),
                    Symbol::new(&fixture.env, "transfer_wrap"),
                    (&fixture.from, &fixture.to, 202403_u64).into_val(&fixture.env),
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        fixture.token_id.clone(),
                        symbol_short!("transfer"),
                        (&fixture.from, &fixture.fee_recipient, 10_i128).into_val(&fixture.env),
                    )),
                    sub_invocations: std::vec![],
                }],
            },
        )],
        "the owner authorization must cover both the wrap and token transfers"
    );

    assert!(client.get_wrap(&fixture.from, &202403).is_none());
    assert_eq!(
        client.get_wrap(&fixture.to, &202403).unwrap(),
        original,
        "a transfer must preserve the immutable wrap data and mint timestamp"
    );
    assert_eq!(client.balance_of(&fixture.from), 2);
    assert_eq!(client.balance_of(&fixture.to), 2);
    assert_eq!(
        client.get_latest_wrap(&fixture.from).unwrap().period,
        202402
    );
    assert_eq!(client.get_latest_wrap(&fixture.to).unwrap().period, 202405);
    assert_eq!(token.balance(&fixture.from), 90);
    assert_eq!(token.balance(&fixture.fee_recipient), 10);
}

#[test]
#[should_panic]
fn transfer_requires_owner_authorization() {
    let fixture = fixture(Some(10), 100);
    mint(&fixture, &fixture.from, 202401, 1);
    fixture.env.set_auths(&[]);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).transfer_wrap(
        &fixture.from,
        &fixture.to,
        &202401,
    );
}

#[test]
fn failed_fee_payment_rolls_back_the_transfer() {
    let fixture = fixture(Some(10), 0);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);

    assert!(
        client
            .try_transfer_wrap(&fixture.from, &fixture.to, &202401)
            .is_err(),
        "the token contract must reject an unfunded fee payment"
    );
    assert!(client.get_wrap(&fixture.from, &202401).is_some());
    assert!(client.get_wrap(&fixture.to, &202401).is_none());
    assert_eq!(client.balance_of(&fixture.from), 1);
    assert_eq!(client.balance_of(&fixture.to), 0);
}

#[test]
fn destination_collision_does_not_charge_a_fee() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    let token = TokenClient::new(&fixture.env, &fixture.token_id);
    mint(&fixture, &fixture.from, 202401, 1);
    mint(&fixture, &fixture.to, 202401, 2);

    assert!(client
        .try_transfer_wrap(&fixture.from, &fixture.to, &202401)
        .is_err());
    assert_eq!(token.balance(&fixture.from), 100);
    assert_eq!(token.balance(&fixture.fee_recipient), 0);
    assert!(client.get_wrap(&fixture.from, &202401).is_some());
    assert!(client.get_wrap(&fixture.to, &202401).is_some());
}

#[test]
fn zero_fee_allows_transfer_without_a_token_balance() {
    let fixture = fixture(Some(0), 0);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);

    client.transfer_wrap(&fixture.from, &fixture.to, &202401);

    assert!(client.get_wrap(&fixture.from, &202401).is_none());
    assert!(client.get_wrap(&fixture.to, &202401).is_some());
    assert_eq!(client.balance_of(&fixture.from), 0);
    assert_eq!(client.balance_of(&fixture.to), 1);
    assert!(client.get_latest_wrap(&fixture.from).is_none());
    assert_eq!(client.get_latest_wrap(&fixture.to).unwrap().period, 202401);
    assert_eq!(
        client.get_transfer_fee(),
        Some(TransferFeeConfig {
            amount: 0,
            recipient: fixture.fee_recipient,
            token: fixture.token_id,
        })
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #48)")]
fn transfer_rejects_missing_fee_configuration() {
    let fixture = fixture(None, 100);
    mint(&fixture, &fixture.from, 202401, 1);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).transfer_wrap(
        &fixture.from,
        &fixture.to,
        &202401,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn transfer_rejects_a_missing_source_record() {
    let fixture = fixture(Some(10), 100);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).transfer_wrap(
        &fixture.from,
        &fixture.to,
        &202401,
    );
}

#[test]
fn admin_can_backfill_legacy_period_indexes() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);
    mint(&fixture, &fixture.from, 202402, 2);

    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(fixture.from.clone()));
    });

    client.backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64, 202402_u64]);
    client.transfer_wrap(&fixture.from, &fixture.to, &202402);

    assert_eq!(client.balance_of(&fixture.from), 1);
    assert_eq!(
        client.get_latest_wrap(&fixture.from).unwrap().period,
        202401
    );
    assert!(client.get_wrap(&fixture.to, &202402).is_some());
}

#[test]
fn legacy_owner_must_be_backfilled_before_another_mint() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);

    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(fixture.from.clone()));
    });

    let archetype = symbol_short!("arch");
    let data_hash = BytesN::from_array(&fixture.env, &[2; 32]);
    let signature = sign_payload(
        &fixture.env,
        &fixture.signing_key,
        &fixture.contract_id,
        &fixture.from,
        202402,
        &archetype,
        &data_hash,
    );
    assert!(client
        .try_mint_wrap(
            &fixture.from,
            &202402,
            &archetype,
            &data_hash,
            &1u32,
            &signature
        )
        .is_err());
    assert!(client.get_wrap(&fixture.from, &202402).is_none());
    assert_eq!(client.balance_of(&fixture.from), 1);

    client.backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64]);
    client.mint_wrap(
        &fixture.from,
        &202402,
        &archetype,
        &data_hash,
        &1u32,
        &signature,
    );
    assert_eq!(client.balance_of(&fixture.from), 2);
}

#[test]
fn transfer_rejects_a_missing_legacy_index_without_charging() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    let token = TokenClient::new(&fixture.env, &fixture.token_id);
    mint(&fixture, &fixture.from, 202401, 1);

    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(fixture.from.clone()));
    });

    assert!(client
        .try_transfer_wrap(&fixture.from, &fixture.to, &202401)
        .is_err());
    assert!(client.get_wrap(&fixture.from, &202401).is_some());
    assert!(client.get_wrap(&fixture.to, &202401).is_none());
    assert_eq!(token.balance(&fixture.from), 100);
    assert_eq!(token.balance(&fixture.fee_recipient), 0);
}

#[test]
fn backfill_rejects_wrong_count_duplicates_and_missing_records() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);
    mint(&fixture, &fixture.from, 202402, 2);

    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(fixture.from.clone()));
    });

    assert!(client
        .try_backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64])
        .is_err());
    assert!(client
        .try_backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64, 202401_u64],)
        .is_err());
    assert!(client
        .try_backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64, 202403_u64],)
        .is_err());

    client.backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64, 202402_u64]);
    assert!(client
        .try_backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64, 202402_u64],)
        .is_err());
}

#[test]
#[should_panic]
fn backfill_requires_admin_authorization() {
    let fixture = fixture(Some(10), 100);
    mint(&fixture, &fixture.from, 202401, 1);
    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .persistent()
            .remove(&DataKey::WrapPeriods(fixture.from.clone()));
    });
    fixture.env.set_auths(&[]);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id)
        .backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401_u64]);
}

#[test]
fn transfer_guard_rejects_nested_transfer_before_fee_collection() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    let token = TokenClient::new(&fixture.env, &fixture.token_id);
    mint(&fixture, &fixture.from, 202401, 1);

    fixture.env.as_contract(&fixture.contract_id, || {
        fixture
            .env
            .storage()
            .temporary()
            .set(&DataKey::TransferGuard, &true);
    });

    assert!(client
        .try_transfer_wrap(&fixture.from, &fixture.to, &202401)
        .is_err());
    assert!(client.get_wrap(&fixture.from, &202401).is_some());
    assert!(client.get_wrap(&fixture.to, &202401).is_none());
    assert_eq!(token.balance(&fixture.from), 100);
    assert_eq!(token.balance(&fixture.fee_recipient), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn admin_cannot_configure_a_negative_fee() {
    let fixture = fixture(Some(10), 100);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).set_transfer_fee(
        &fixture.token_id,
        &fixture.fee_recipient,
        &-1,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #49)")]
fn self_transfer_is_rejected() {
    let fixture = fixture(Some(10), 100);
    mint(&fixture, &fixture.from, 202401, 1);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).transfer_wrap(
        &fixture.from,
        &fixture.from,
        &202401,
    );
}

#[test]
#[should_panic]
fn setting_transfer_fee_requires_admin_authorization() {
    let fixture = fixture(Some(10), 100);
    fixture.env.set_auths(&[]);

    StellarWrapContractClient::new(&fixture.env, &fixture.contract_id).set_transfer_fee(
        &fixture.token_id,
        &fixture.fee_recipient,
        &10,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_transfer_wrap_when_paused_fails() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);
    mint(&fixture, &fixture.from, 202401, 1);

    client.pause();
    client.transfer_wrap(&fixture.from, &fixture.to, &202401);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_backfill_wrap_periods_when_paused_fails() {
    let fixture = fixture(Some(10), 100);
    let client = StellarWrapContractClient::new(&fixture.env, &fixture.contract_id);

    client.pause();
    client.backfill_wrap_periods(&fixture.from, &vec![&fixture.env, 202401]);
}

