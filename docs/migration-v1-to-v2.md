# v1 to v2 Integrator Migration Guide

This guide covers the planned split from the deployed v1 monolith to the v2
contract workspace. It is a migration runbook, not a frozen v2 ABI: the v2
architecture RFC (#964) is still open and does not yet define final contract
IDs, entrypoint names, error codes, event schemas, or the state migration
mechanism. Treat every `TBD` below as a release blocker, not as a compatibility
promise. Do not schedule a production cutover until those values are published
in the approved architecture, deployment manifest, and v2 interface references.

The v1 baseline in this guide is the current public interface documented in
`CHANGELOG.md`, `src/lib.rs`, `src/errors.rs`, and the event schema catalogue in
`src/events_test.rs`. The `v1`/`v2` names in
[`signing-payload.md`](signing-payload.md) refer to mint-payload versions; they
do not refer to this contract-architecture migration.

## What changes for integrators

- A v2 deployment uses new contract IDs. The v1 ID is not an alias for the v2
  registry. Configure and record every v2 contract ID by network and role.
- The RFC proposes `wrap-registry`, `wrap-governor`, `wrap-bridge`, and
  `wrap-fees`, plus shared `wrap-types` and `wrap-crypto` crates. These are
  proposed homes, not deployed contract IDs or finalized ABI names.
- Backend signing must bind mint payloads to the v2 registry contract ID.
  Signatures made for the v1 contract ID are not valid for v2; issue fresh
  signatures after routing to the new registry.
- Frontends must use the v2 clients/ABIs and route each operation to its
  owning contract. Do not assume the old monolithic client can be pointed at
  one v2 address.
- Indexers must identify events by both contract ID and schema version, and
  read v1 history alongside v2 events during migration.

## Entrypoint inventory

The RFC proposes the contract homes below. It does not yet assign every v1
entrypoint, nor does it specify renamed or removed methods. The proposed home
is therefore a routing hint only; exact v2 contract and method names remain
`TBD`. No v1 method should be assumed to exist unchanged on v2.

| v1 operation / entrypoints | Proposed v2 home | v2 replacement or disposition |
|---|---|---|
| Mint, revoke, burn, TTL, and registry queries: `mint_wrap`, `mint_wrap_batch`, `revoke_wrap`, `burn_wrap`, `expire_wrap`, `set_expiration_duration`, `get_wrap`, `get_mint_timestamp`, `get_last_updated`, `total_wrap_count`, `get_latest_wrap`, `get_wraps`, `get_all_wraps_for_user`, `balance_of`, `has_wrap`, `verify_data`, `check_user_invariants`, `migration_version` | `wrap-registry` | Exact method names and removed methods: TBD |
| Registry metadata and ownership: `set_wrap_metadata`, `transfer_wrap`, `backfill_wrap_periods`, `transition_wrap_state`, `name`, `symbol`, `decimals`, `set_name`, `set_symbol` | TBD | Confirm whether retained in `wrap-registry`, moved, renamed, or removed |
| Admin, pause, and upgrade: `initialize`, `update_admin`, `update_admin_pubkey`, `pause`, `unpause`, `is_paused`, `migrate`, `upgrade`, `propose_admin`, `accept_admin`, `cancel_proposed_admin`, `get_pending_admin`, `get_admin`, `get_admin_pubkey` | `wrap-governor` is proposed to own the registry admin role | Exact role/call model and methods: TBD; initialization and upgrades are not specified |
| Admin proposals and timelock: `create_admin_proposal`, `vote_admin_proposal`, `execute_admin_proposal`, `cancel_admin_proposal`, `get_admin_proposal`, `get_admin_proposal_vote`, `get_admin_proposal_count`, `enable_timelock`, `timelock_schedule`, `timelock_execute`, `timelock_cancel`, `timelock_delay`, `timelock_operation`, `timelock_pending`, `timelock_operation_id` | `wrap-governor` | Exact method names and proposal/timelock migration: TBD |
| Bridge: `set_bridge_relayer`, `set_bridge_relayers`, `set_chain_status`, `bridge_wrap_out`, `bridge_wrap_refund`, `bridge_wrap_in`, `get_outbound_bridge_request`, `get_inbound_bridge_record`, `is_inbound_nonce_processed`, `get_outbound_nonce`, `is_chain_supported`, `get_bridge_relayers` | `wrap-bridge` | Exact method names, role grants, and bridge-state migration: TBD |
| Fees and staking: `set_transfer_fee`, `clear_transfer_fee`, `get_transfer_fee`, `set_fee_params`, `current_fee`, `fee_params`, `set_stake_config`, `stake`, `unstake`, `withdraw_stake`, `get_stake`, `get_stake_priority`, `total_staked`, `get_stake_config`, `get_discounted_fee` | `wrap-fees` | Exact method names and fee/stake-state migration: TBD |
| Whitelist, aliases, opt-out, oracle, and miscellaneous: `set_whitelist_root`, `clear_whitelist_root`, `get_whitelist_root`, `whitelist_leaf`, `verify_whitelist`, `set_alias_hash`, `get_alias_hash`, `opt_out`, `opt_in`, `is_opted_out`, `health`, `storage_bytes`, `total_revoked` | TBD | Assign a v2 home or explicitly mark removed before cutover |

Before release, replace this inventory with a method-by-method approved table
that identifies each v1 method as retained, renamed (with its replacement),
removed (with the supported alternative), or split across contracts. Include
the generated v2 client method and required role/authentication for every
retained operation.

## Error-code mapping

The v1 numeric codes are the `ContractError` discriminants in
[`src/errors.rs`](../src/errors.rs). v2 error numbers and variants have not
been specified. Numeric parity is not a compatibility guarantee: translate
errors by the v2 contract ID plus its documented variant, and map those
variants to stable application-level categories. The table is the complete v1
inventory; fill in the v2 column from the approved v2 error registry before
shipping clients.

| v1 code | v1 variant | v2 code / variant |
|---:|---|---|
| 1 | `AlreadyInitialized` | TBD |
| 2 | `NotInitialized` | TBD |
| 3 | `Unauthorized` | TBD |
| 4 | `WrapAlreadyExists` | TBD |
| 5 | `InvalidSignature` | TBD |
| 6 | `InvalidPeriod` | TBD |
| 7 | `MigrationAlreadyApplied` | TBD |
| 8 | `InvalidStateTransition` | TBD |
| 9 | `WrapNotFound` | TBD |
| 10 | `NoAdminTransferProposal` | TBD |
| 11 | `AdminTransferProposalExists` | TBD |
| 12 | `Paused` | TBD |
| 13 | `ArithmeticOverflow` | TBD |
| 14 | `InvalidFeeParams` | TBD |
| 15 | `BatchEmpty` | TBD |
| 16 | `BatchTooLarge` | TBD |
| 17 | `DuplicateBatchEntry` | TBD |
| 18 | `StakeTooLow` | TBD |
| 19 | `StakeNotFound` | TBD |
| 20 | `StakeCooldownActive` | TBD |
| 21 | `StakeNotUnstaking` | TBD |
| 22 | `StakeCooldownNotElapsed` | TBD |
| 23 | `InvalidStakeConfig` | TBD |
| 24 | `StakeArithmeticOverflow` | TBD |
| 25 | `ProposalNotFound` | TBD |
| 26 | `ProposalNotActive` | TBD |
| 27 | `ProposalAlreadyVoted` | TBD |
| 28 | `ProposalVotingPeriodNotEnded` | TBD |
| 29 | `ProposalVotingPeriodEnded` | TBD |
| 30 | `ProposalDefeated` | TBD |
| 31 | `InvalidProposalDuration` | TBD |
| 32 | `UserOptedOut` | TBD |
| 33 | `BridgeNotInitialized` | TBD |
| 34 | `InvalidChain` | TBD |
| 35 | `ChainDisabled` | TBD |
| 36 | `NonceAlreadyProcessed` | TBD |
| 37 | `InvalidBridgePayload` | TBD |
| 38 | `MerkleRootNotSet` | TBD |
| 39 | `InvalidMerkleProof` | TBD |
| 40 | `TimelockNotReady` | TBD |
| 41 | `TimelockOperationNotFound` | TBD |
| 42 | `TimelockOperationExists` | TBD |
| 43 | `InvalidTimelockDelay` | TBD |
| 44 | `TimelockRequired` | TBD |
| 45 | `TimelockAlreadyEnabled` | TBD |
| 46 | `WrapNotExpired` | TBD |
| 47 | `InvalidExpirationDuration` | TBD |
| 48 | `TransferFeeAlreadyConfigured` | TBD |
| 49 | `InvalidTransfer` | TBD |
| 50 | `TransferInProgress` | TBD |
| 51 | `StorageInvariantViolation` | TBD |
| 52 | `InvalidAdminPubKey` | TBD |
| 53 | `InvalidThreshold` | TBD |
| 54 | `MerkleProofTooLong` | TBD |
| 55 | `TimelockOperationExpired` | TBD |
| 56 | `TimelockOperationNotExpired` | TBD |
| 57 | `BridgeRequestNotPending` | TBD |
| 58 | `BridgeRefundDelayNotElapsed` | TBD |
| 59 | `BridgeRequestAlreadyRefunded` | TBD |
| 60 | `StaleProposal` | TBD |
| 61 | `InvalidRelayerSet` | TBD |
| 62 | `SignatureExpired` | TBD |

Keep application behavior stable even if v2 separates errors by contract. At
minimum, preserve distinct UI/retry handling for authorization, validation,
already-exists/not-found, paused, expired-signature, and retryable
bridge/timelock conditions. The current mint payload version is a separate
versioning concern; see [`signing-payload.md`](signing-payload.md).

## Event mapping and indexer compatibility

The v1 shapes below are pinned by [`src/events_test.rs`](../src/events_test.rs)
and the current pause behavior is also recorded in `CHANGELOG.md`. No v2 event
topics or data layouts have been approved, so every v2 mapping remains `TBD`.
Keep decoders versioned by source contract/schema, not by event name alone.

| v1 event family | v1 topic/data shape (summary) | v2 event / decoder mapping |
|---|---|---|
| Initialization and admin | `v1/admin/init`; `v1/admin/updated`; `v1/pubkey/rotate` | TBD |
| Pause state | Current changelog: `pause/paused` or `pause/unpaused`, data is acting admin. Older v1 history may contain `v1/admin/pause`, data is `paused: bool`. | TBD; retain both historical v1 decoders |
| Fee configuration | `fee` with `(token, recipient, amount)`; `fee_clr` with empty data | TBD |
| Upgrade | `upgrade/<version>` with WASM hash; timelocked upgrades also emit `upgrade` with WASM hash | TBD |
| Wrap mint | `v1/wrap/mint` with user, period, archetype; batch mint uses `Mint/<user>/<period>` and mint data | TBD |
| Wrap expiry | `expire/<user>/<period>` with `expired` marker | TBD |
| Transfer and metadata | `transfer/<from>/<to>/<period>` with empty data or fee tuple; `backfill/<user>` with period count; metadata event carries user, period, description, image URL | TBD |
| Revoke and burn | `revoke/<user>/<period>` with reason hash; `burn/<user>/<period>` with user | TBD |
| Staking | `stake/<user>/init`, `stake/<user>/add`, `unstake/<user>`, `withdraw/<user>`, `stake/cfg` | TBD |
| Timelock | `timelock/enabled`, `timelock/sched`, `timelock/cancel`, `timelock/exec`, `timelock/sweep`; upgrade/admin actions have dedicated topics | TBD |
| Governance | `gov/propose`, `gov/vote`, `gov/executed`, `gov/defeated`, `gov/cancelled` | TBD |
| Bridge | `br_out/<user>/<destination_chain>`, `br_refund/<sender>/<period>`, `br_in/<recipient>/<source_chain>` | TBD |

During the dual-read period:

1. Load all v1 and v2 contract IDs for each network from a versioned deployment
   manifest. Keep contract ID, network, and decoder/schema version on every
   indexed event.
2. Decode v1 and v2 independently (`decodeV1` / `decodeV2`) into the same
   domain model. Preserve source identity and ledger ordering; do not silently
   discard unknown versions or topics. Log and count them for alerting.
3. Backfill v1 history and consume v2 from its deployment ledger. Reconcile
   per-user state against contract queries before switching product reads.
   Define deduplication and record-lineage rules before enabling any v2 state
   import, since the migration mechanism is not yet specified.
4. Keep the v1 decoder available for historical replay after live v1 reads end.
   End dual live reads only after the cutover gates below pass; do not erase
   v1 history or its contract ID from the manifest.

## Cutover timeline

| Phase | Integrator actions | Exit gate |
|---|---|---|
| Freeze | Pin current v1 client/ABI and deployment manifest; inventory every frontend call, backend signer/job, admin operation, event consumer, and stored contract ID. Avoid unrelated interface changes during the migration window. | Complete owner-by-owner inventory and tested v1 rollback path |
| Migrate | Deploy v2 contracts to a non-production network; populate the approved method/error/event maps and contract IDs; implement per-contract clients, signing against the v2 registry ID, role configuration, v1/v2 decoders, and any approved state migration. | End-to-end tests pass for each role and operation; v1 history and v2 state reconcile |
| Switch | Publish the production deployment manifest; route frontend/backend writes to v2; route reads and indexers through the combined v1/v2 view; monitor failed calls, unknown events, and state discrepancies. | All traffic uses intended v2 IDs, no unexplained parity gaps, and rollback remains available |
| Deprecate | Disable new v1 writes only after v2 health and reconciliation gates pass; retain v1 read access and historical decoding for the agreed retention period; remove v1 clients only in a later release. | Owners approve end of live dual-read window and archive/retention requirements are met |

There are no calendar dates in this guide. Maintainers should publish phase
dates and a rollback owner with the production deployment manifest. The exact
state migration/bridge strategy and the duration of live dual reads must be
approved before the `Migrate` phase begins.

## Go-live checklist

- [ ] Approved v2 architecture maps every v1 entrypoint to a named v2 method or an explicit removal and replacement.
- [ ] Published deployment manifest lists every v1/v2 contract ID, network, role, and activation ledger.
- [ ] Published error registry completes the v1-to-v2 mapping above; clients handle errors per contract and semantic category.
- [ ] Published event schema completes the v1-to-v2 mapping above; indexer fixtures cover historical v1 and deployed v2 events.
- [ ] Backend signing uses the v2 registry ID and the documented v2 payload; stale v1 signatures are rejected and refreshed.
- [ ] State migration, reconciliation, deduplication, rollback, and live dual-read exit criteria are tested and owned.
- [ ] Frontend, backend, operator tooling, and indexer all pass the same testnet cutover rehearsal.
- [ ] The final cutover date, monitoring thresholds, and deprecation date are announced to integrators.