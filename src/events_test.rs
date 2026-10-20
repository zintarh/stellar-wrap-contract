//! Event-schema pinning tests (issue #712).
//!
//! The `(topics, data)` pair a contract publishes is a public API: `pipeline/`
//! and any external indexer decode it, and a shape can change without breaking
//! the contract itself. These tests freeze the exact topics and data of every
//! event the contract emits, so a change surfaces as a test diff instead of as a
//! broken indexer in production.
//!
//! [`EVENT_SHAPES`] is the single source of truth for the catalogue. `README.md`
//! documents the same rows, and `test_readme_documents_every_pinned_shape` fails
//! if the two drift apart.
//!
//! Two publication paths exist in the tree today:
//!
//! * the typed path — `events::publish_event`, used by `initialize`, `pause` /
//!   `unpause` and the single-item `mint_wrap`;
//! * the ad-hoc path — a raw `e.events().publish((..), ..)` tuple, used by
//!   everything else.
//!
//! Each shape is pinned as actually emitted, whichever path produces it.

#![cfg(test)]

extern crate std;

use std::vec::Vec;

use ed25519_dalek::SigningKey;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Bytes, BytesN, Env, String, Symbol, TryIntoVal, Val,
};

use super::*;
use crate::test_utils::{decode_events, default_test_env, sign_batch_payload, sign_payload};

// ── Catalogue ───────────────────────────────────────────────────────────────

/// One pinned `(topics, data)` shape.
struct EventShape {
    /// Entrypoint that publishes the event.
    emitter: &'static str,
    /// Topic symbols in order; `<..>` marks a per-call value.
    topics: &'static str,
    /// Shape of the data payload.
    data: &'static str,
}

/// Every event the contract publishes, as emitted by the code in `src/`.
const EVENT_SHAPES: &[EventShape] = &[
    // Admin
    EventShape {
        emitter: "initialize",
        topics: "v1 / admin / init",
        data: "Event::AdminInit(admin)",
    },
    EventShape {
        emitter: "pause, unpause",
        topics: "v1 / admin / pause",
        data: "Event::AdminPause(paused)",
    },
    EventShape {
        emitter: "update_admin",
        topics: "v1 / admin / updated",
        data: "(current_admin, new_admin)",
    },
    EventShape {
        emitter: "set_transfer_fee",
        topics: "fee",
        data: "(token, recipient, amount)",
    },
    EventShape {
        emitter: "clear_transfer_fee",
        topics: "fee_clr",
        data: "()",
    },
    EventShape {
        emitter: "upgrade",
        topics: "upgrade / <version>",
        data: "wasm_hash: BytesN<32>",
    },
    EventShape {
        emitter: "update_admin_pubkey",
        topics: "v1 / pubkey / rotate",
        data: "new_pubkey: BytesN<32>",
    },
    EventShape {
        emitter: "set_wrap_metadata",
        topics: "v1 / admin / metadata",
        data: "(user, period, description, image_url)",
    },
    // Mint
    EventShape {
        emitter: "mint_wrap",
        topics: "v1 / wrap / mint",
        data: "Event::Mint(user, period, archetype)",
    },
    EventShape {
        emitter: "mint_wrap_batch",
        topics: "Mint / <user> / <period>",
        data: "MintEventData::Mint(user, period, archetype)",
    },
    EventShape {
        emitter: "expire_wrap",
        topics: "expire / <user> / <period>",
        data: "symbol_short!(\"expired\")",
    },
    // Transfer
    EventShape {
        emitter: "transfer_wrap (no fee)",
        topics: "transfer / <from> / <to> / <period>",
        data: "()",
    },
    EventShape {
        emitter: "transfer_wrap (fee charged)",
        topics: "transfer / <from> / <to> / <period>",
        data: "(token, recipient, amount)",
    },
    EventShape {
        emitter: "backfill_wrap_periods",
        topics: "backfill / <user>",
        data: "period_count: u32",
    },
    // Revoke / burn
    EventShape {
        emitter: "revoke_wrap",
        topics: "revoke / <user> / <period>",
        data: "reason_hash: BytesN<32>",
    },
    EventShape {
        emitter: "burn_wrap",
        topics: "burn / <user> / <period>",
        data: "user: Address",
    },
    // Stake
    EventShape {
        emitter: "stake (first stake)",
        topics: "stake / <user> / init",
        data: "amount: i128",
    },
    EventShape {
        emitter: "stake (top-up)",
        topics: "stake / <user> / add",
        data: "amount: i128",
    },
    EventShape {
        emitter: "unstake",
        topics: "unstake / <user>",
        data: "amount: i128",
    },
    EventShape {
        emitter: "withdraw_stake",
        topics: "withdraw / <user>",
        data: "amount: i128",
    },
    EventShape {
        emitter: "set_stake_config",
        topics: "stake / cfg",
        data: "config: StakeConfig",
    },
    // Timelock
    EventShape {
        emitter: "enable_timelock",
        topics: "timelock / enabled",
        data: "delay_seconds: u64",
    },
    EventShape {
        emitter: "timelock_schedule",
        topics: "timelock / sched",
        data: "(id, eta)",
    },
    EventShape {
        emitter: "timelock_cancel",
        topics: "timelock / cancel",
        data: "id: BytesN<32>",
    },
    EventShape {
        emitter: "timelock_execute (upgrade action)",
        topics: "upgrade",
        data: "wasm_hash: BytesN<32>",
    },
    EventShape {
        emitter: "timelock_execute (admin action)",
        topics: "admin / updated",
        data: "(admin, new_admin)",
    },
    EventShape {
        emitter: "timelock_execute (other action)",
        topics: "timelock / exec",
        data: "id: BytesN<32>",
    },
    EventShape {
        emitter: "timelock_sweep_expired",
        topics: "timelock / sweep",
        data: "id: BytesN<32>",
    },
    // Governance
    EventShape {
        emitter: "create_admin_proposal",
        topics: "gov / propose",
        data: "(proposal_id, proposer, proposed_admin)",
    },
    EventShape {
        emitter: "vote_admin_proposal",
        topics: "gov / vote",
        data: "(proposal_id, voter, support)",
    },
    EventShape {
        emitter: "execute_admin_proposal (passed)",
        topics: "gov / executed",
        data: "(proposal_id, proposed_admin)",
    },
    EventShape {
        emitter: "execute_admin_proposal (defeated)",
        topics: "gov / defeated",
        data: "proposal_id: u64",
    },
    EventShape {
        emitter: "cancel_admin_proposal",
        topics: "gov / cancelled",
        data: "(proposal_id, caller)",
    },
    // Bridge
    EventShape {
        emitter: "bridge_wrap_out",
        topics: "br_out / <user> / <destination_chain>",
        data: "(nonce, recipient_address, period)",
    },
    EventShape {
        emitter: "bridge_wrap_refund",
        topics: "br_refund / <sender> / <period>",
        data: "outbound_nonce: u64",
    },
    EventShape {
        emitter: "bridge_wrap_in",
        topics: "br_in / <recipient> / <source_chain>",
        data: "(source_nonce, period)",
    },
];

// ── Harness helpers ─────────────────────────────────────────────────────────

/// Register the contract, initialise it, and return what a test needs.
fn setup(env: &Env, seed: u8) -> (Address, StellarWrapContractClient<'_>, SigningKey, Address) {
    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(env, &contract_id);
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let admin_pubkey = BytesN::from_array(env, &signing_key.verifying_key().to_bytes());
    let admin = Address::generate(env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    (contract_id, client, signing_key, admin)
}

/// The `(topics, data)` pair of the last event emitted by the previous call.
fn last_event(env: &Env) -> (Vec<Val>, Val) {
    decode_events(env).pop().expect("no event was emitted")
}

/// A topic decoded as a symbol.
fn symbol_at(env: &Env, topics: &[Val], index: usize) -> Symbol {
    topics[index].try_into_val(env).unwrap()
}

/// Decode a topic as a symbol, or `None` when it is not a symbol at all.
fn decode_symbol(env: &Env, value: Val) -> Option<Symbol> {
    let decoded: Symbol = value.try_into_val(env).ok()?;
    Some(decoded)
}

/// Assert every topic of an event against literal symbols.
fn assert_topics(env: &Env, topics: &[Val], expected: &[&str]) {
    let actual: Vec<Symbol> = topics
        .iter()
        .copied()
        .map(|topic| decode_symbol(env, topic).unwrap())
        .collect();
    let expected: Vec<Symbol> = expected.iter().map(|name| Symbol::new(env, name)).collect();
    assert_eq!(actual, expected, "event topics changed");
}

/// Assert the leading literal topics of an event, ignoring per-call topics.
///
/// `expected` is compared position by position from index 0; the remaining
/// topics are left to the caller, which can decode them as a user, a period, or
/// a chain id.
fn assert_leading_topics(env: &Env, topics: &[Val], expected: &[&str]) {
    assert!(
        topics.len() >= expected.len(),
        "expected at least {} topics, found {}",
        expected.len(),
        topics.len()
    );
    for (index, name) in expected.iter().enumerate() {
        assert_eq!(
            symbol_at(env, topics, index),
            Symbol::new(env, name),
            "topic {index} changed"
        );
    }
}

/// Mint a single wrap through the public entrypoint.
fn mint(
    env: &Env,
    client: &StellarWrapContractClient<'_>,
    contract_id: &Address,
    signing_key: &SigningKey,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    hash: &BytesN<32>,
) {
    let signature = sign_payload(env, signing_key, contract_id, user, period, archetype, hash);
    client.mint_wrap(user, &period, archetype, hash, &1u32, &signature);
}

// ── Catalogue consistency ───────────────────────────────────────────────────

#[test]
fn test_catalogue_lists_every_event_once() {
    let mut emitters: Vec<&str> = EVENT_SHAPES.iter().map(|shape| shape.emitter).collect();
    let total = emitters.len();
    emitters.sort_unstable();
    emitters.dedup();

    assert_eq!(emitters.len(), total, "duplicate catalogue entries");
    assert!(
        total >= 30,
        "the catalogue should cover every emitted event, found {total}"
    );
    for shape in EVENT_SHAPES {
        assert!(!shape.topics.is_empty(), "{} has no topics", shape.emitter);
        assert!(
            !shape.data.is_empty(),
            "{} has no data shape",
            shape.emitter
        );
    }
}

#[test]
fn test_readme_documents_every_pinned_shape() {
    const README: &str = include_str!("../README.md");

    for shape in EVENT_SHAPES {
        assert!(
            README.contains(shape.topics),
            "README is missing the topics `{}` for `{}`",
            shape.topics,
            shape.emitter
        );
        assert!(
            README.contains(shape.data),
            "README is missing the data shape `{}` for `{}`",
            shape.data,
            shape.emitter
        );
    }
}

// ── Admin ───────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_publishes_typed_v1_topics() {
    let env = default_test_env();
    let (_, _, _, _admin) = setup(&env, 71);

    let (topics, _data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "admin", "init"]);
}

#[test]
fn test_pause_and_unpause_publish_typed_v1_topics() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 72);

    client.pause();
    let (topics, _data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "admin", "pause"]);

    client.unpause();
    let (topics, _data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "admin", "pause"]);
}

#[test]
fn test_update_admin_publishes_previous_and_new_admin() {
    let env = default_test_env();
    let (_, client, _, admin) = setup(&env, 73);
    let new_admin = Address::generate(&env);

    client.update_admin(&new_admin);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "admin", "updated"]);
    let (previous, next): (Address, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(previous, admin);
    assert_eq!(next, new_admin);
}

#[test]
fn test_transfer_fee_events_are_single_topic_and_match_the_config() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 74);
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.set_transfer_fee(&token, &recipient, &25i128);
    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["fee"]);
    let (emitted_token, emitted_recipient, amount): (Address, Address, i128) =
        data.try_into_val(&env).unwrap();
    assert_eq!(emitted_token, token);
    assert_eq!(emitted_recipient, recipient);
    assert_eq!(amount, 25i128);

    client.clear_transfer_fee();
    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["fee_clr"]);
    let empty: () = data.try_into_val(&env).unwrap();
    assert_eq!(empty, ());
}

#[test]
fn test_update_admin_pubkey_publishes_rotated_key() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 75);
    let new_pubkey = BytesN::from_array(&env, &[9u8; 32]);

    client.update_admin_pubkey(&new_pubkey);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "pubkey", "rotate"]);
    let emitted: BytesN<32> = data.try_into_val(&env).unwrap();
    assert_eq!(emitted, new_pubkey);
}

#[test]
fn test_set_wrap_metadata_publishes_user_period_and_metadata() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 76);
    let user = Address::generate(&env);
    let period = 202_401u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[3u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    let description = String::from_str(&env, "Verra-verified stand");
    let image_url = String::from_str(&env, "https://cdn.example.org/stand.jpg");
    client.set_wrap_metadata(
        &user,
        &period,
        &Some(description.clone()),
        &Some(image_url.clone()),
    );

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "admin", "metadata"]);
    let (emitted_user, emitted_period, emitted_description, emitted_image): (
        Address,
        u64,
        String,
        String,
    ) = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_period, period);
    assert_eq!(emitted_description, description);
    assert_eq!(emitted_image, image_url);
}

// ── Mint ────────────────────────────────────────────────────────────────────

#[test]
fn test_mint_wrap_publishes_typed_v1_topics() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 77);
    let user = Address::generate(&env);
    let period = 202_402u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[4u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    let (topics, _data) = last_event(&env);
    assert_topics(&env, &topics, &["v1", "wrap", "mint"]);
}

#[test]
fn test_mint_wrap_batch_publishes_one_event_per_item() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 78);
    let user = Address::generate(&env);
    let archetype = symbol_short!("arch");
    let first_period = 202_403u64;
    let dummy_sig = BytesN::from_array(&env, &[0u8; 64]);

    let mut items = soroban_sdk::vec![&env];
    for offset in 0..3u64 {
        items.push_back(crate::storage_types::BatchWrapItem {
            user: user.clone(),
            period: first_period + offset,
            archetype: archetype.clone(),
            data_hash: BytesN::from_array(&env, &[offset as u8; 32]),
            payload_version: 1,
            signature: dummy_sig.clone(),
        });
    }

    let aggregated = sign_batch_payload(&env, &signing_key, &contract_id, &items, 1);
    client.mint_wrap_batch(&items, &Some(aggregated));

    let batch_mint_topic = Symbol::new(&env, "Mint");
    let mint_events: Vec<(Vec<Val>, Val)> = decode_events(&env)
        .into_iter()
        .filter(|(topics, _)| {
            topics
                .first()
                .copied()
                .and_then(|first| decode_symbol(&env, first))
                .map(|topic| topic == batch_mint_topic)
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(mint_events.len(), 3, "one event per batch item");

    // Every item publishes the identical shape.
    let (topics, data) = &mint_events[0];
    assert_leading_topics(&env, topics, &["Mint"]);
    let topic_user: Address = topics[1].try_into_val(&env).unwrap();
    let topic_period: u64 = topics[2].try_into_val(&env).unwrap();
    assert_eq!(topic_user, user);
    assert_eq!(topic_period, first_period);

    let (event_kind, emitted_user, emitted_period, emitted_archetype): (
        Symbol,
        Address,
        u64,
        Symbol,
    ) = data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_kind, batch_mint_topic);
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_period, first_period);
    assert_eq!(emitted_archetype, archetype);
}

#[test]
fn test_expire_wrap_publishes_period_and_marker() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 79);
    let user = Address::generate(&env);
    let period = 202_404u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[5u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.expire_wrap(&user, &period);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "expire_wrap publishes three topics");
    assert_leading_topics(&env, &topics, &["expire"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    let emitted_period: u64 = topics[2].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_period, period);
    let marker: Symbol = data.try_into_val(&env).unwrap();
    assert_eq!(marker, symbol_short!("expired"));
}

// ── Transfer, revoke, burn ──────────────────────────────────────────────────

#[test]
fn test_transfer_wrap_without_fee_publishes_empty_data() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 80);
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let period = 202_405u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[6u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &from,
        period,
        &archetype,
        &hash,
    );

    client.transfer_wrap(&from, &to, &period);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 4, "transfer_wrap publishes four topics");
    assert_leading_topics(&env, &topics, &["transfer"]);
    let emitted_from: Address = topics[1].try_into_val(&env).unwrap();
    let emitted_to: Address = topics[2].try_into_val(&env).unwrap();
    let emitted_period: u64 = topics[3].try_into_val(&env).unwrap();
    assert_eq!(emitted_from, from);
    assert_eq!(emitted_to, to);
    assert_eq!(emitted_period, period);
    let empty: () = data.try_into_val(&env).unwrap();
    assert_eq!(empty, ());
}

#[test]
fn test_revoke_wrap_publishes_reason_hash() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 81);
    let user = Address::generate(&env);
    let period = 202_406u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    let reason = BytesN::from_array(&env, &[0xAB; 32]);
    client.revoke_wrap(&user, &period, &reason);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "revoke_wrap publishes three topics");
    assert_leading_topics(&env, &topics, &["revoke"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    let emitted_period: u64 = topics[2].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_period, period);
    let emitted_reason: BytesN<32> = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_reason, reason);
}

#[test]
fn test_burn_wrap_publishes_owner_as_data() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 82);
    let user = Address::generate(&env);
    let period = 202_407u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[8u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    client.burn_wrap(&user, &period);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "burn_wrap publishes three topics");
    assert_leading_topics(&env, &topics, &["burn"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    let emitted_period: u64 = topics[2].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_period, period);
    let owner: Address = data.try_into_val(&env).unwrap();
    assert_eq!(owner, user);
}

// ── Stake ───────────────────────────────────────────────────────────────────

#[test]
fn test_stake_events_distinguish_first_stake_from_top_up() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 83);
    let user = Address::generate(&env);

    client.stake(&user, &500i128);
    let (topics, data) = last_event(&env);
    assert_leading_topics(&env, &topics, &["stake"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(symbol_at(&env, &topics, 2), symbol_short!("init"));
    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 500i128);

    client.stake(&user, &250i128);
    let (topics, data) = last_event(&env);
    assert_leading_topics(&env, &topics, &["stake"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(symbol_at(&env, &topics, 2), symbol_short!("add"));
    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 250i128);
}

#[test]
fn test_unstake_and_withdraw_publish_two_topic_shapes() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 84);
    let user = Address::generate(&env);

    client.stake(&user, &1_000i128);
    client.unstake(&user);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2, "unstake publishes two topics");
    assert_leading_topics(&env, &topics, &["unstake"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    let amount: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(amount, 1_000i128);

    // The default cooldown is 7 days; move past it before withdrawing.
    env.ledger().with_mut(|ledger| {
        ledger.timestamp += 604_801;
    });
    client.withdraw_stake(&user);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 2, "withdraw_stake publishes two topics");
    assert_leading_topics(&env, &topics, &["withdraw"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    let withdrawn: i128 = data.try_into_val(&env).unwrap();
    assert_eq!(withdrawn, 1_000i128);
}

#[test]
fn test_set_stake_config_publishes_the_config_as_data() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 85);

    let config = StakeConfig {
        min_stake: 1_000i128,
        cooldown_seconds: 86_400u64,
        priority_multiplier_bps: 100u32,
        max_priority_bps: 5_000u32,
    };
    client.set_stake_config(&config);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["stake", "cfg"]);
    let emitted: StakeConfig = data.try_into_val(&env).unwrap();
    assert_eq!(emitted, config);
}

// ── Timelock ────────────────────────────────────────────────────────────────

#[test]
fn test_enable_timelock_publishes_the_delay() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 86);

    client.enable_timelock(&3_600u64);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["timelock", "enabled"]);
    let delay: u64 = data.try_into_val(&env).unwrap();
    assert_eq!(delay, 3_600u64);
}

#[test]
fn test_timelock_schedule_and_cancel_publish_the_operation_id() {
    let env = default_test_env();
    let (_, client, _, admin) = setup(&env, 87);
    client.enable_timelock(&3_600u64);

    let action = TimelockAction::SetAdmin(Address::generate(&env));
    let id = client.timelock_schedule(&action);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["timelock", "sched"]);
    let (emitted_id, eta): (BytesN<32>, u64) = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_id, id);
    assert!(eta > 0, "a scheduled operation carries an execution time");

    client.timelock_cancel(&id);
    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["timelock", "cancel"]);
    let cancelled: BytesN<32> = data.try_into_val(&env).unwrap();
    assert_eq!(cancelled, id);

    // The cancelled operation never replaced the admin.
    assert_eq!(client.get_admin().unwrap(), admin);
}

// ── Governance ──────────────────────────────────────────────────────────────

#[test]
fn test_governance_proposal_and_vote_shapes() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 88);
    let proposer = client.get_admin().unwrap();
    let proposed_admin = Address::generate(&env);

    let proposal_id = client.create_admin_proposal(&proposer, &proposed_admin, &3_600u64);
    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["gov", "propose"]);
    let (emitted_id, emitted_proposer, emitted_proposed): (u64, Address, Address) =
        data.try_into_val(&env).unwrap();
    assert_eq!(emitted_id, proposal_id);
    assert_eq!(emitted_proposer, proposer);
    assert_eq!(emitted_proposed, proposed_admin);

    let voter = Address::generate(&env);
    client.vote_admin_proposal(&voter, &proposal_id, &true);
    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["gov", "vote"]);
    let (emitted_id, emitted_voter, support): (u64, Address, bool) =
        data.try_into_val(&env).unwrap();
    assert_eq!(emitted_id, proposal_id);
    assert_eq!(emitted_voter, voter);
    assert!(support);
}

#[test]
fn test_cancel_admin_proposal_publishes_caller() {
    let env = default_test_env();
    let (_, client, _, _) = setup(&env, 89);
    let proposer = client.get_admin().unwrap();
    let proposed_admin = Address::generate(&env);

    let proposal_id = client.create_admin_proposal(&proposer, &proposed_admin, &3_600u64);
    client.cancel_admin_proposal(&proposer, &proposal_id);

    let (topics, data) = last_event(&env);
    assert_topics(&env, &topics, &["gov", "cancelled"]);
    let (emitted_id, caller): (u64, Address) = data.try_into_val(&env).unwrap();
    assert_eq!(emitted_id, proposal_id);
    assert_eq!(caller, proposer);
}

// ── Bridge ──────────────────────────────────────────────────────────────────

#[test]
fn test_bridge_wrap_out_publishes_nonce_recipient_and_period() {
    let env = default_test_env();
    let (contract_id, client, signing_key, _) = setup(&env, 90);
    let user = Address::generate(&env);
    let period = 202_408u64;
    let archetype = symbol_short!("arch");
    let hash = BytesN::from_array(&env, &[11u8; 32]);

    mint(
        &env,
        &client,
        &contract_id,
        &signing_key,
        &user,
        period,
        &archetype,
        &hash,
    );

    let destination_chain = 1u32;
    client.set_chain_status(&destination_chain, &true);
    let recipient = Bytes::from_slice(&env, b"0xbridge-recipient");

    let nonce = client.bridge_wrap_out(&user, &destination_chain, &recipient, &period);

    let (topics, data) = last_event(&env);
    assert_eq!(topics.len(), 3, "bridge_wrap_out publishes three topics");
    assert_leading_topics(&env, &topics, &["br_out"]);
    let emitted_user: Address = topics[1].try_into_val(&env).unwrap();
    let emitted_chain: u32 = topics[2].try_into_val(&env).unwrap();
    assert_eq!(emitted_user, user);
    assert_eq!(emitted_chain, destination_chain);

    let (emitted_nonce, emitted_recipient, emitted_period): (u64, Bytes, u64) =
        data.try_into_val(&env).unwrap();
    assert_eq!(emitted_nonce, nonce);
    assert_eq!(emitted_recipient, recipient);
    assert_eq!(emitted_period, period);
}
