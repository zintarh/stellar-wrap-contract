# Changelog

## Unreleased

### ⚠️ Breaking Changes

<!-- Future breaking interface changes go here. Each entry must include
     migration notes describing what integrators need to update. -->

### Changed

- `pause` / `unpause` now emit direction-distinguishable events with the acting
  admin as the payload, instead of a shared `("pause",)` topic carrying a
  boolean payload:
  - pause:   topic `("pause", "paused")`, data = acting admin
  - unpause: topic `("pause", "unpaused")`, data = acting admin
- A redundant `pause`/`unpause` that requests the state already in effect is a
  silent no-op and no longer emits an event, so alerting systems only see
  signals that correspond to an actual state change.

## 0.1.0

### Contract interface snapshot

This release documents the current public Soroban interface for the Stellar Wrap contract and records the client-facing contract changes that should be considered by backend and frontend integrations.

#### Write methods

- `initialize(e, admin, admin_pubkey)`
- `update_admin(e, new_admin)`
- `update_admin_pubkey(e, new_pubkey)`
- `pause(e)` / `unpause(e)`
- `migrate(e, version)`
- `mint_wrap(e, user, period, archetype, data_hash, payload_version, signature)`
- `mint_wrap_batch(e, items, aggregated_signature)`
- `set_wrap_metadata(e, user, period, description, image_url)`
- `transfer_wrap(e, from, to, period)`
- `backfill_wrap_periods(e, user, periods)`
- `transition_wrap_state(e, user, period, next_state)`
- `set_expiration_duration(e, duration)`
- `expire_wrap(e, user, period)`
- `set_transfer_fee(e, token, recipient, amount)`
- `clear_transfer_fee(e)`
- `set_name(e, name)`
- `set_symbol(e, symbol)`
- `upgrade(e, new_wasm_hash)`
- `propose_admin(e, new_admin)`
- `accept_admin(e)`
- `cancel_proposed_admin(e)`
- `set_whitelist_root(e, root)`
- `clear_whitelist_root(e)`
- `set_alias_hash(e, user, alias_hash)`
- `opt_out(e, user)`
- `opt_in(e, user)`
- `set_stake_config(e, config)`
- `stake(e, user, amount)`
- `unstake(e, user)`
- `withdraw_stake(e, user)`
- `set_bridge_relayer(e, relayer)`
- `set_bridge_relayers(e, chain_id, relayers, threshold)`
- `set_chain_status(e, chain_id, enabled)`
- `bridge_wrap_out(e, user, destination_chain, recipient_address, period)`
- `bridge_wrap_refund(e, outbound_nonce)`
- `bridge_wrap_in(e, source_chain, source_nonce, recipient, period, archetype, data_hash, signatures)`
- `enable_timelock(e, delay_seconds)`
- `timelock_schedule(e, action)`
- `timelock_execute(e, id)`
- `timelock_cancel(e, id)`
- `revoke_wrap(e, user, period, reason_hash)`
- `burn_wrap(e, user, period)`
- `set_fee_params(e, params)`

#### Read methods

- `get_wrap(e, user, period)`
- `get_mint_timestamp(e, user, period)`
- `get_last_updated(e, user)`
- `balance_of(e, user)`
- `total_wrap_count(e)`
- `verify_data(e, user, period, data)`
- `get_latest_wrap(e, user)`
- `get_wraps(e, user, start, limit)`
- `get_all_wraps_for_user(e, user)`
- `check_user_invariants(e, user)`
- `has_wrap(e, user, period)`
- `get_admin(e)`
- `get_admin_pubkey(e)`
- `get_transfer_fee(e)`
- `health(e)`
- `name(e)`, `symbol(e)`, `decimals(e)`
- `migration_version(e)`
- `version(e)`
- `contract_version(e)`
- `expiration_duration(e)`
- `get_stake(e, user)`
- `get_stake_priority(e, user)`
- `total_staked(e)`
- `get_stake_config(e)`
- `get_discounted_fee(e, user)`
- `get_outbound_bridge_request(e, nonce)`
- `get_inbound_bridge_record(e, source_chain, source_nonce)`
- `is_inbound_nonce_processed(e, source_chain, source_nonce)`
- `get_outbound_nonce(e)`
- `get_pending_admin(e)`
- `is_paused(e)`
- `is_opted_out(e, user)`
- `get_whitelist_root(e)`
- `whitelist_leaf(e, user)`
- `verify_whitelist(e, user, proof)`
- `get_alias_hash(e, user)`
- `get_admin_proposal(e, proposal_id)`
- `get_admin_proposal_vote(e, proposal_id, voter)`
- `get_admin_proposal_count(e)`
- `timelock_delay(e)`
- `timelock_operation(e, id)`
- `timelock_pending(e)`
- `timelock_operation_id(e, action)`
- `storage_bytes(e)`
- `current_fee(e)`
- `fee_params(e)`
- `total_revoked(e)`
- `is_chain_supported(e, chain_id)`
- `get_bridge_relayers(e, chain_id)`

### ⚠️ Breaking Changes

The following changes distinguish this interface from the initial release and require integrator attention.

#### Versioned mint signatures

Mint signatures are now versioned. Clients must sign the canonical payload with the current payload-versioning scheme and pass the `payload_version` argument to `mint_wrap`. Legacy integrations that only prepared signatures for the older layout should update their signer flow before deployment.

#### Expanded query surface

The documented public interface now includes additional query helpers such as `get_mint_timestamp`, `total_wrap_count`, `get_wraps`, `get_last_updated`, `get_all_wraps_for_user`, `check_user_invariants`, `has_wrap`, `get_admin_pubkey`, `get_transfer_fee`, `get_stake_*`, `get_outbound_bridge_request`, `get_inbound_bridge_record`, and `timelock_*`. Consumers that rely on older assumptions about the contract surface should review the current method list before building or updating clients.

#### Non-transferable wraps and revoke semantics

Wraps remain non-transferable and revoke semantics are not implemented as a standard token burn. Frontends and indexers should use contract queries such as `get_wrap`, `balance_of`, and `verify_data` rather than inferring state from events alone.

#### Governance, staking, and bridging

The contract now exposes a DAO governance layer (`create_admin_proposal`, `vote_admin_proposal`, `execute_admin_proposal`, `cancel_admin_proposal`), a staking layer (`stake`, `unstake`, `withdraw_stake`, `get_stake_priority`, `get_discounted_fee`), and a cross-chain bridge layer (`bridge_wrap_out`, `bridge_wrap_in`, `bridge_wrap_refund`, `set_bridge_relayers`, `set_chain_status`). Integrations that previously assumed a minimal surface must account for these additional entrypoints.

#### Timelock controller

A timelock controller (`enable_timelock`, `timelock_schedule`, `timelock_execute`, `timelock_cancel`) is now available. After enabling, admin handover, key rotation, WASM upgrades, and whitelist-root changes must go through the timelock. Integrations that perform these operations directly will need to update their workflows.

### Migration notes

For every future release, include a `### ⚠️ Breaking Changes` subsection describing any breaking interface change and the required updates for client code, backend signers, and indexers.
