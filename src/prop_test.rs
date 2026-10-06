//! Property-based tests for the StellarWrap contract.

#![cfg(test)]
#![allow(clippy::manual_is_multiple_of)]
extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Symbol};

use super::*;
use crate::{merkle, mint::CURRENT_PAYLOAD_VERSION, signature::construct_mint_payload};

// ── Shared test constants ────────────────────────────────────────────────────

const TEST_SIGNING_KEY_BYTES: [u8; 32] = [0xAB; 32];

// Admin proposal duration bounds, mirroring the timelock delay bounds.
const MIN_PROPOSAL_DURATION: u64 = 3600;
const MAX_PROPOSAL_DURATION: u64 = 30 * 24 * 60 * 60;

// ── Helper: allowed archetypes ────────────────────────────────────────────────

fn allowed_archetype_symbols() -> std::vec::Vec<&'static str> {
    std::vec!["builder", "arch", "architect", "soroban", "defi", "patron"]
}

// ── Helper: environment setup ─────────────────────────────────────────────────

fn setup_env() -> (
    Env,
    StellarWrapContractClient<'static>,
    Address,
    SigningKey,
    [u8; 32],
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(StellarWrapContract, ());
    let client = StellarWrapContractClient::new(&env, &contract_id);

    let signing_key = SigningKey::from_bytes(&TEST_SIGNING_KEY_BYTES);
    let pubkey_bytes = signing_key.verifying_key().to_bytes();
    let admin_pubkey = BytesN::from_array(&env, &pubkey_bytes);
    let admin = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin, &admin_pubkey);

    (env, client, contract_id, signing_key, pubkey_bytes)
}

// ── Helper: build a valid non-zero data hash ─────────────────────────────────

fn make_data_hash(env: &Env, raw: [u8; 32]) -> BytesN<32> {
    if raw == [0u8; 32] {
        BytesN::from_array(env, &[0x01u8; 32])
    } else {
        BytesN::from_array(env, &raw)
    }
}

// ── Helper: sign a canonical mint payload ────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn sign_mint(
    env: &Env,
    signing_key: &SigningKey,
    contract: &Address,
    user: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> BytesN<64> {
    let payload = construct_mint_payload(
        env,
        contract,
        user,
        period,
        archetype,
        data_hash,
        payload_version,
        u64::MAX,
    );

    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);

    let sig = signing_key.sign(&buf[..len]);
    BytesN::from_array(env, &sig.to_bytes())
}

// ── Helper: Merkle reference tree builder and proof generator ────────────────

fn build_reference_merkle_root(e: &Env, leaves: &[BytesN<32>]) -> BytesN<32> {
    assert!(!leaves.is_empty(), "leaves cannot be empty");
    let mut layer: std::vec::Vec<BytesN<32>> = leaves.to_vec();
    while layer.len() > 1 {
        let mut next = std::vec::Vec::new();
        for i in (0..layer.len()).step_by(2) {
            if i + 1 < layer.len() {
                next.push(merkle::hash_pair(e, &layer[i], &layer[i + 1]));
            } else {
                next.push(layer[i].clone());
            }
        }
        layer = next;
    }
    layer[0].clone()
}

fn build_reference_merkle_proof(
    e: &Env,
    leaves: &[BytesN<32>],
    index: usize,
) -> soroban_sdk::Vec<BytesN<32>> {
    assert!(index < leaves.len(), "index out of bounds");
    let mut proof = soroban_sdk::Vec::new(e);
    let mut idx = index;
    let mut layer: std::vec::Vec<BytesN<32>> = leaves.to_vec();
    while layer.len() > 1 {
        let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        if sibling_idx < layer.len() {
            proof.push_back(layer[sibling_idx].clone());
        }
        let mut next = std::vec::Vec::new();
        for i in (0..layer.len()).step_by(2) {
            if i + 1 < layer.len() {
                next.push(merkle::hash_pair(e, &layer[i], &layer[i + 1]));
            } else {
                next.push(layer[i].clone());
            }
        }
        idx /= 2;
        layer = next;
    }
    proof
}

// ── Proptest strategies ───────────────────────────────────────────────────────

fn arb_period() -> impl Strategy<Value = u64> {
    (2024u64..=2100u64, 1u64..=12u64).prop_map(|(year, month)| year * 100 + month)
}

fn arb_data_hash() -> impl Strategy<Value = [u8; 32]> {
    any::<[u8; 32]>().prop_map(|b| if b == [0u8; 32] { [0x01u8; 32] } else { b })
}

fn arb_archetype() -> impl Strategy<Value = &'static str> {
    prop::sample::select(allowed_archetype_symbols())
}

// ── Property Tests ───────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_valid_mint_is_retrievable(
        period in arb_period(),
        raw_hash in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let data_hash = make_data_hash(&env, raw_hash);
        let archetype = Symbol::new(&env, archetype_str);

        let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash, CURRENT_PAYLOAD_VERSION);

        client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

        let record = client.get_wrap(&user, &period)
            .expect("get_wrap must return Some after successful mint");

        prop_assert_eq!(record.data_hash, data_hash);
        prop_assert_eq!(record.archetype, archetype);
        prop_assert_eq!(record.period, period);
    }
}

proptest! {
    #[test]
    fn prop_balance_equals_mint_count(
        mut periods in proptest::collection::vec(arb_period(), 1..=8usize),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        periods.sort_unstable();
        periods.dedup();

        for (k, &period) in periods.iter().enumerate() {
            let data_hash = make_data_hash(&env, [(k as u8).wrapping_add(1); 32]);

            let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash, CURRENT_PAYLOAD_VERSION);

            client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

            let expected_balance = (k as i128) + 1;
            prop_assert_eq!(client.balance_of(&user), expected_balance);
        }
    }
}

proptest! {
    #[test]
    fn prop_duplicate_period_always_fails(
        period in arb_period(),
        raw_hash_first in arb_data_hash(),
        raw_hash_second in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        let data_hash_first = make_data_hash(&env, raw_hash_first);
        let sig_first = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash_first, CURRENT_PAYLOAD_VERSION);
        client.mint_wrap(&user, &period, &archetype, &data_hash_first, &CURRENT_PAYLOAD_VERSION, &sig_first);

        let data_hash_second = make_data_hash(&env, raw_hash_second);
        let sig_second = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash_second, CURRENT_PAYLOAD_VERSION);

        let result = client.try_mint_wrap(&user, &period, &archetype, &data_hash_second, &CURRENT_PAYLOAD_VERSION, &sig_second);
        prop_assert!(result.is_err());

        let stored = client.get_wrap(&user, &period).expect("original wrap must still exist");
        prop_assert_eq!(stored.data_hash, data_hash_first);
    }
}

proptest! {
    #[test]
    fn prop_invalid_signature_always_rejected(
        period in arb_period(),
        raw_hash in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let data_hash = make_data_hash(&env, raw_hash);
        let archetype = Symbol::new(&env, archetype_str);

        let mut sig_bytes = sign_mint(
            &env,
            &signing_key,
            &contract_id,
            &user,
            period,
            &archetype,
            &data_hash,
            CURRENT_PAYLOAD_VERSION,
        )
        .to_array();
        sig_bytes[0] ^= 0xFF;
        let bad_sig = BytesN::from_array(&env, &sig_bytes);

        let result = client.try_mint_wrap(
            &user,
            &period,
            &archetype,
            &data_hash,
            &CURRENT_PAYLOAD_VERSION,
            &u64::MAX,
            &bad_sig,
        );
        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_balance_is_monotonically_increasing(
        mut periods in proptest::collection::vec(arb_period(), 2..=6usize),
        archetype_str in arb_archetype(),
    ) {
        periods.sort_unstable();
        periods.dedup();
        prop_assume!(periods.len() >= 2);

        let (env, client, contract_id, signing_key, _) = setup_env();

        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);

        let mut prev_balance: i128 = 0;

        for (k, &period) in periods.iter().enumerate() {
            let data_hash = make_data_hash(&env, [(k as u8).wrapping_add(2); 32]);
            let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash, CURRENT_PAYLOAD_VERSION);

            client.mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &u64::MAX, &sig);

            let new_balance = client.balance_of(&user);
            prop_assert!(new_balance > prev_balance);
            prev_balance = new_balance;
        }
    }
}

proptest! {
    #[test]
    fn prop_get_wrap_returns_none_for_unminted_period(
        period in arb_period(),
    ) {
        let (env, client, _, _, _) = setup_env();
        let user = Address::generate(&env);

        prop_assert!(client.get_wrap(&user, &period).is_none());
    }
}

proptest! {
    #[test]
    fn prop_merkle_proof_verification(
        (tree_size, member_idx) in (1..=64usize).prop_flat_map(|size| (Just(size), 0..size)),
    ) {
        let env = Env::default();
        let addresses: std::vec::Vec<Address> = (0..tree_size).map(|_| Address::generate(&env)).collect();
        let leaves: std::vec::Vec<BytesN<32>> = addresses
            .iter()
            .map(|addr| merkle::compute_whitelist_leaf(&env, addr))
            .collect();

        let root = build_reference_merkle_root(&env, &leaves);
        let member_leaf = leaves[member_idx].clone();
        let proof = build_reference_merkle_proof(&env, &leaves, member_idx);

        // 1. Correct proof for any member verifies
        prop_assert!(merkle::verify_merkle_proof(&env, &root, &member_leaf, &proof));

        // 2. A proof with any single sibling mutated fails
        for s_idx in 0..proof.len() {
            let orig = proof.get(s_idx).unwrap();
            let mut mutated_bytes = orig.to_array();
            mutated_bytes[0] ^= 0xFF;
            let mutated_sibling = BytesN::from_array(&env, &mutated_bytes);
            let mut bad_proof = proof.clone();
            bad_proof.set(s_idx, mutated_sibling);
            prop_assert!(!merkle::verify_merkle_proof(&env, &root, &member_leaf, &bad_proof));
        }

        // 3. A proof with siblings reordered fails
        if proof.len() >= 2 {
            let mut rev_proof = soroban_sdk::Vec::new(&env);
            for i in (0..proof.len()).rev() {
                rev_proof.push_back(proof.get(i).unwrap());
            }
            prop_assert!(!merkle::verify_merkle_proof(&env, &root, &member_leaf, &rev_proof));

            if proof.len() > 2 {
                let mut rot_proof = soroban_sdk::Vec::new(&env);
                for i in 1..proof.len() {
                    rot_proof.push_back(proof.get(i).unwrap());
                }
                rot_proof.push_back(proof.get(0).unwrap());
                prop_assert!(!merkle::verify_merkle_proof(&env, &root, &member_leaf, &rot_proof));
            }
        }

        // 4. A proof for a non-member fails
        let non_member = Address::generate(&env);
        if !addresses.contains(&non_member) {
            let non_member_leaf = merkle::compute_whitelist_leaf(&env, &non_member);
            prop_assert!(!merkle::verify_merkle_proof(&env, &root, &non_member_leaf, &proof));
        }
        if tree_size > 1 {
            let other_idx = (member_idx + 1) % tree_size;
            let other_leaf = leaves[other_idx].clone();
            prop_assert!(!merkle::verify_merkle_proof(&env, &root, &other_leaf, &proof));
        }

        // 5. A proof padded with extra siblings fails (second-preimage extension)
        let extra_sibling = BytesN::from_array(&env, &[0xDE; 32]);
        let mut padded_proof = proof.clone();
        padded_proof.push_back(extra_sibling.clone());
        prop_assert!(!merkle::verify_merkle_proof(&env, &root, &member_leaf, &padded_proof));

        let mut prepended_proof = soroban_sdk::Vec::new(&env);
        prepended_proof.push_back(extra_sibling);
        for s in proof.iter() {
            prepended_proof.push_back(s);
        }
        prop_assert!(!merkle::verify_merkle_proof(&env, &root, &member_leaf, &prepended_proof));
    }
}

proptest! {
    #[test]
    fn prop_whitelist_contract_verification(
        (tree_size, member_idx) in (1..=32usize).prop_flat_map(|size| (Just(size), 0..size)),
    ) {
        let (env, client, _, _, _) = setup_env();
        let addresses: std::vec::Vec<Address> = (0..tree_size).map(|_| Address::generate(&env)).collect();
        let leaves: std::vec::Vec<BytesN<32>> = addresses
            .iter()
            .map(|addr| merkle::compute_whitelist_leaf(&env, addr))
            .collect();

        let root = build_reference_merkle_root(&env, &leaves);
        client.set_whitelist_root(&root);
        prop_assert_eq!(client.get_whitelist_root(), Some(root.clone()));

        let member = &addresses[member_idx];
        let proof = build_reference_merkle_proof(&env, &leaves, member_idx);

        prop_assert!(client.verify_whitelist(member, &proof));

        let non_member = Address::generate(&env);
        if !addresses.contains(&non_member) {
            prop_assert!(!client.verify_whitelist(&non_member, &proof));
        }
    }
}

proptest! {
    #[test]
    fn prop_invalid_proposal_duration_is_rejected(
        duration in prop_oneof![
            Just(0u64),
            Just(MIN_PROPOSAL_DURATION - 1),
            Just(MAX_PROPOSAL_DURATION + 1),
            Just(u64::MAX),
        ],
    ) {
        let (env, client, _, _, _) = setup_env();
        let title = String::from_str(&env, "proposal");
        let description = String::from_str(&env, "duration bounds");
        let data_hash = make_data_hash(&env, [0x42; 32]);

        let result = client.try_create_admin_proposal(
            &title,
            &description,
            &data_hash,
            &duration,
        );

        prop_assert!(result.is_err());
    }
}

proptest! {
    #[test]
    fn prop_boundary_proposal_durations_are_accepted(
        duration in prop_oneof![
            Just(MIN_PROPOSAL_DURATION),
            Just(MAX_PROPOSAL_DURATION),
        ],
    ) {
        let (env, client, _, _, _) = setup_env();
        let title = String::from_str(&env, "proposal");
        let description = String::from_str(&env, "duration bounds");
        let data_hash = make_data_hash(&env, [0x43; 32]);

        let result = client.try_create_admin_proposal(
            &title,
            &description,
            &data_hash,
            &duration,
        );

        prop_assert!(result.is_ok());
    }
}

// ── Bridge Invariant Property Test ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BridgeOp {
    BridgeOut,
    BridgeIn,
    Refund,
    Revoke,
    Burn,
}

fn arb_bridge_op() -> impl Strategy<Value = BridgeOp> {
    prop_oneof![
        Just(BridgeOp::BridgeOut),
        Just(BridgeOp::BridgeIn),
        Just(BridgeOp::Refund),
        Just(BridgeOp::Revoke),
        Just(BridgeOp::Burn),
    ]
}

#[allow(clippy::too_many_arguments)]
fn sign_inbound(
    env: &Env,
    signing_key: &SigningKey,
    contract: &Address,
    source_chain: u32,
    source_nonce: u64,
    recipient: &Address,
    period: u64,
    archetype: &Symbol,
    data_hash: &BytesN<32>,
    payload_version: u32,
) -> BytesN<64> {
    let payload = crate::signature::construct_inbound_bridge_payload(
        env,
        contract,
        source_chain,
        source_nonce,
        recipient,
        period,
        archetype,
        data_hash,
        payload_version,
    );

    let mut buf = [0u8; 512];
    let len = payload.len() as usize;
    payload.copy_into_slice(&mut buf[..len]);

    let sig = signing_key.sign(&buf[..len]);
    BytesN::from_array(env, &sig.to_bytes())
}

proptest! {
    #[test]
    fn prop_cross_chain_supply_invariant(
        ops in proptest::collection::vec(arb_bridge_op(), 1..=20usize),
        period in arb_period(),
        raw_hash in arb_data_hash(),
        archetype_str in arb_archetype(),
    ) {
        let (env, client, contract_id, signing_key, _) = setup_env();
        let user = Address::generate(&env);
        let archetype = Symbol::new(&env, archetype_str);
        let data_hash = make_data_hash(&env, raw_hash);
        
        let relayer_signing_key = SigningKey::from_bytes(&[0xBB; 32]);
        let relayer_pubkey = BytesN::from_array(&env, &relayer_signing_key.verifying_key().to_bytes());
        let relayer_addr = Address::generate(&env);
        
        client.set_chain_status(&1, &true);
        let relayers = soroban_sdk::Vec::from_array(&env, [relayer_pubkey.clone()]);
        client.set_bridge_relayers(&1, &relayers, &1u32);
        client.set_bridge_relayer(&relayer_addr);

        let sig = sign_mint(&env, &signing_key, &contract_id, &user, period, &archetype, &data_hash, CURRENT_PAYLOAD_VERSION);
        let _ = client.try_mint_wrap(&user, &period, &archetype, &data_hash, &CURRENT_PAYLOAD_VERSION, &sig);

        let mut expected_supply: i32 = 1;
        let mut outstanding_outbound: i32 = 0;
        let mut active_nonce: Option<u64> = None;
        let mut bridge_in_counter = 1000u64;

        for op in ops {
            match op {
                BridgeOp::BridgeOut => {
                    let recipient = BytesN::from_array(&env, &[0; 32]).into();
                    if let Ok(nonce) = client.try_bridge_wrap_out(&user, &1u32, &recipient, &period) {
                        active_nonce = Some(nonce);
                        outstanding_outbound += 1;
                    }
                }
                BridgeOp::Refund => {
                    if let Some(nonce) = active_nonce {
                        if client.try_bridge_wrap_refund(&nonce).is_ok() {
                            active_nonce = None;
                            outstanding_outbound -= 1;
                        }
                    }
                }
                BridgeOp::BridgeIn => {
                    bridge_in_counter += 1;
                    let source_nonce = bridge_in_counter;
                    let in_sig = sign_inbound(
                        &env, &relayer_signing_key, &contract_id, 1, source_nonce, &user, period, 
                        &archetype, &data_hash, crate::signature::INBOUND_BRIDGE_SCHEME_VERSION
                    );
                    let sigs = soroban_sdk::Vec::from_array(&env, [in_sig]);
                    
                    if client.try_bridge_wrap_in(&1u32, &source_nonce, &user, &period, &archetype, &data_hash, &sigs).is_ok() {
                        outstanding_outbound -= 1;
                        active_nonce = None;
                    }
                }
                BridgeOp::Revoke => {
                    let mut was_active = false;
                    if let Some(wrap) = client.get_wrap(&user, &period) {
                        if wrap.fsm.state == crate::storage_types::WrapState::Active {
                            was_active = true;
                        }
                    }
                    let reason = BytesN::from_array(&env, &[0; 32]);
                    if client.try_revoke_wrap(&user, &period, &reason).is_ok() {
                        if was_active {
                            expected_supply -= 1;
                        }
                    }
                }
                BridgeOp::Burn => {
                    let mut was_active = false;
                    if let Some(wrap) = client.get_wrap(&user, &period) {
                        if wrap.fsm.state == crate::storage_types::WrapState::Active {
                            was_active = true;
                        }
                    }
                    if client.try_burn_wrap(&user, &period).is_ok() {
                        if was_active {
                            expected_supply -= 1;
                        }
                    }
                }
            }
            
            let mut local_active: i32 = 0;
            if let Some(wrap) = client.get_wrap(&user, &period) {
                if wrap.fsm.state == crate::storage_types::WrapState::Active {
                    local_active = 1;
                }
            }
            
            prop_assert_eq!(local_active + outstanding_outbound, expected_supply, "Invariant failed: local_active ({}) + outstanding_outbound ({}) != expected_supply ({})", local_active, outstanding_outbound, expected_supply);
        }
    }
}
