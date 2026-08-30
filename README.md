# Stellar Wrap Contract

[![Coverage](https://codecov.io/gh/zintarh/stellar-wrap-contract/branch/main/graph/badge.svg)](https://codecov.io/gh/zintarh/stellar-wrap-contract)

Soroban contract for storing non-transferable Stellar Wrap records by wallet and reporting successful wrap mints through events.

## Changelog and client migration notes

The current contract interface for version 0.1.0 is documented in [CHANGELOG.md](CHANGELOG.md). Backend and frontend consumers should review the migration notes there before updating integrations, especially around the versioned mint-signature payload and the expanded query surface.

## Contract layout

The contract is split into focused modules:

- `src/lib.rs`: contract type and module wiring
- `src/admin.rs`: initialization and admin updates
- `src/mint.rs`: period validation, signature verification, wrap minting, event emission
- `src/bridge.rs`: generic token bridge interface for cross-chain wrap interactions
- `src/queries.rs`: read-only queries and metadata
- `src/errors.rs`: contract error codes
- `src/storage_types.rs`: storage keys and persisted record types
- `src/test_utils.rs`: shared test-only helpers (e.g. payload signing)

For detailed bridge architecture and cross-chain workflow, see [docs/bridge-architecture.md](docs/bridge-architecture.md).

## Data model

### `WrapRecord`

Each wrap record stores:

- `timestamp: u64`
- `data_hash: BytesN<32>`
- `archetype: Symbol`
- `period: u64`

`period` is encoded as `YYYYMM` and validated on mint:

- year must be between `2024` and `2100`
- month must be between `01` and `12`

## SBT compatibility

Wrap records are implemented as non-transferable (soulbound) entries. The contract intentionally omits `transfer`, `transfer_from`, `approve`, and `allowance` methods. As a result:

- `balance_of(user)` returns the number of wrap records minted for `user`, not a tradable token balance.
- records cannot be transferred between addresses by users.
- any future removal or replacement of a wrap record would require an admin-controlled operation, not a user-initiated transfer.
### `ContractHealth`

Returned by `health()`, reports:

- `initialized: bool` — whether `initialize()` has been called
- `has_admin: bool` — whether an admin address is currently configured
- `has_signing_key: bool` — whether an admin signing key is currently configured

## Storage keys

- `DataKey::Admin`
- `DataKey::AdminPubKey`
- `DataKey::Wrap(Address, u64)`
- `DataKey::WrapCount(Address)`
- `DataKey::LatestPeriod(Address)`
- `DataKey::MigrationVersion`

## API reference

Every public entrypoint in `src/lib.rs` is listed below, grouped by module. The
auth column states who must authorise the invocation ("—" = permissionless
read). See the subsystem sections that follow for the authorization model,
worked examples, and links to the detailed docs.

### Administration & lifecycle

| Entrypoint | Auth |
| --- | --- |
| `initialize(admin, admin_pubkey)` | deployer (callable once) |
| `update_admin(new_admin)` | admin |
| `propose_admin(new_admin)` | admin |
| `accept_admin()` | proposed admin |
| `cancel_proposed_admin()` | admin |
| `get_pending_admin()` | — |
| `get_admin()` | — |
| `get_admin_pubkey()` | — |
| `set_name(name)` | admin |
| `set_symbol(symbol)` | admin |
| `pause()` / `unpause()` | admin |
| `is_paused()` | — |
| `migrate(version)` | admin |
| `migration_version()` | — |
| `upgrade(new_wasm_hash)` | admin |
| `renew_all_ttls(user)` | admin |
| `extend_ttl(user, period)` | anyone |
| `set_transfer_fee(token, recipient, amount)` | admin |
| `get_transfer_fee()` | — |
| `set_expiration_duration(duration)` | admin |
| `expiration_duration()` | — |

### Minting

| Entrypoint | Auth |
| --- | --- |
| `mint_wrap(user, period, archetype, data_hash, payload_version, signature)` | `user` + admin Ed25519 signature |
| `mint_wrap_batch(items, aggregated_signature)` | per-item `user` + signature (see [Batch minting](#batch-minting)) |
| `transition_wrap_state(user, period, next_state)` | `user` |
| `expire_wrap(user, period)` | anyone |

### Mint signature payload versioning

The contract requires mint signatures over a canonical, versioned payload. The
current payload (`CURRENT_PAYLOAD_VERSION = 1`) is the byte-level concatenation
of:

- `MINT_DOMAIN_SEPARATOR` — the raw ASCII bytes `"stellar-wrap-v1"` (15 bytes, **not** XDR-encoded)
- `XDR(payload_version)` — a `u32` that must equal `1`
- `XDR(contract_id)`
- `XDR(user)`
- `XDR(period)`
- `XDR(archetype)`
- `XDR(data_hash)`

`mint_wrap` rejects any `payload_version` other than the current one with
`Error(Contract, #5)` (`InvalidSignature`) *before* verifying the signature, so
clients must sign this exact byte layout. See
[docs/signing-payload.md](docs/signing-payload.md) for the full reference and a
TypeScript signer example.

### Transfer

| Entrypoint | Auth |
| --- | --- |
| `transfer_wrap(from, to, period)` | `from` (charges the configured fee) |
| `backfill_wrap_periods(user, periods)` | admin |

### Queries

| Entrypoint |
| --- |
| `get_wrap(user, period)` |
| `get_mint_timestamp(user, period)` |
| `get_last_updated(user)` |
| `total_wrap_count()` |
| `get_latest_wrap(user)` |
| `get_wraps(user, start, limit)` |
| `get_all_wraps_for_user(user)` |
| `has_wrap(user, period)` |
| `version()` |
| `contract_version()` |
| `health()` |

`get_wrap` returns the wrap record for the specified user and period. It is
safe to call before initialization — it returns `None` if the contract has not
been initialized or if no wrap exists for the given user and period.

### Data verification

| Entrypoint |
| --- |
| `verify_data(user, period, data)` |
| `verify_with_oracle(oracle, data_hash)` |

### Alias & opt-out

| Entrypoint | Auth |
| --- | --- |
| `set_alias_hash(user, alias_hash)` | `user` |
| `get_alias_hash(user)` | — |
| `opt_out(user)` / `opt_in(user)` | `user` |
| `is_opted_out(user)` | — |

### Revoke & burn

| Entrypoint | Auth |
| --- | --- |
| `revoke_wrap(user, period, reason_hash)` | admin |
| `burn_wrap(user, period)` | wrap owner |
| `total_revoked()` | — |

### Storage accounting & fees

| Entrypoint | Auth |
| --- | --- |
| `storage_bytes()` | — |
| `current_fee()` | — |
| `set_fee_params(params)` | admin |
| `fee_params()` | — |

### Whitelist (Merkle)

| Entrypoint | Auth |
| --- | --- |
| `set_whitelist_root(root)` | admin |
| `clear_whitelist_root()` | admin |
| `get_whitelist_root()` | — |
| `whitelist_leaf(user)` | — |
| `verify_whitelist(user, proof)` | — |

### Timelock

| Entrypoint | Auth |
| --- | --- |
| `enable_timelock(delay_seconds)` | admin (one-way) |
| `timelock_delay()` | — |
| `timelock_schedule(action)` | admin |
| `timelock_execute(id)` | admin |
| `timelock_cancel(id)` | admin |
| `timelock_operation(id)` | — |
| `timelock_pending()` | — |
| `timelock_operation_id(action)` | — |

### Bridge

| Entrypoint | Auth |
| --- | --- |
| `set_bridge_relayer(relayer)` | admin |
| `get_bridge_relayer()` | — |
| `set_chain_status(chain_id, enabled)` | admin |
| `is_chain_supported(chain_id)` | — |
| `bridge_wrap_out(user, destination_chain, recipient_address, period)` | `user` |
| `bridge_wrap_in(source_chain, source_nonce, recipient, period, archetype, data_hash)` | relayer |
| `get_outbound_bridge_request(nonce)` | — |
| `get_inbound_bridge_record(source_chain, source_nonce)` | — |
| `is_inbound_nonce_processed(source_chain, source_nonce)` | — |
| `get_outbound_nonce()` | — |

### DAO governance

| Entrypoint | Auth |
| --- | --- |
| `create_admin_proposal(proposer, proposed_admin, duration_seconds)` | proposer |
| `vote_admin_proposal(voter, proposal_id, support)` | voter |
| `execute_admin_proposal(proposal_id)` | anyone after the voting period ends |
| `cancel_admin_proposal(caller, proposal_id)` | proposer or admin |
| `get_admin_proposal(proposal_id)` | — |
| `get_admin_proposal_vote(proposal_id, voter)` | — |
| `get_admin_proposal_count()` | — |

### Staking

| Entrypoint | Auth |
| --- | --- |
| `stake(user, amount)` | `user` |
| `unstake(user)` | `user` |
| `withdraw_stake(user)` | `user` |
| `get_stake(user)` | — |
| `get_stake_priority(user)` | — |
| `total_staked()` | — |
| `set_stake_config(config)` | admin |
| `get_stake_config()` | — |
| `get_discounted_fee(user)` | — |

### Token interface (SEP-41)

| Entrypoint |
| --- |
| `name()` |
| `symbol()` |
| `decimals()` |
| `balance_of(user)` |

## Staking

Users stake the contract token to earn a mint-fee discount ("priority"). The
discount is expressed in basis points and derived from the stake size relative
to `min_stake`. Staking is opt-in and per-user; a cooldown applies before
staked funds can be withdrawn.

### Entrypoints

| Entrypoint | Auth | Purpose |
| --- | --- | --- |
| `stake(user, amount)` | `user` | Deposit at least `min_stake`; raises priority. |
| `unstake(user)` | `user` | Start the cooldown; priority drops to 0 immediately. |
| `withdraw_stake(user)` | `user` | Withdraw once the cooldown has elapsed. |
| `get_stake(user)` | — | The user's `StakeRecord` (amount, timestamps). |
| `get_stake_priority(user)` | — | Discount priority in basis points (0 while unstaking). |
| `total_staked()` | — | Sum of all active stakes. |
| `set_stake_config(config)` | admin | Configure `min_stake`, cooldown, and the priority curve. |
| `get_stake_config()` | — | The current `StakeConfig`. |
| `get_discounted_fee(user)` | — | The raw fee reduced by the user's priority discount. |

### Authorization model

- `stake`, `unstake`, and `withdraw_stake` require the **user's** authorization
  (`user.require_auth()`).
- `set_stake_config` is **admin-only**; the config is validated on write
  (`min_stake > 0`, `cooldown_seconds > 0`, `max_priority_bps <= 10_000`).
- All reads are permissionless.

### Worked example

1. Admin configures the curve: `set_stake_config({ min_stake: 100, cooldown_seconds: 604800, priority_multiplier_bps: 1000, max_priority_bps: 5000 })`.
2. Alice calls `stake(alice, 500)`.
3. `get_stake_priority(alice)` returns `min(500 / 100 * 1000, 5000) = 5000` bps (50%).
4. `get_discounted_fee(alice)` returns the storage fee reduced by 50%.
5. Alice calls `unstake(alice)`, waits 7 days, then `withdraw_stake(alice)`.

> **Not yet enforced:** the staking discount is *computed* (`get_discounted_fee`)
> but is **not applied** during `mint_wrap`/`mint_wrap_batch` — mints do not
> currently charge the storage fee, so staking changes the priority number only,
> not the amount a user actually pays.

## Timelock controller

`enable_timelock` is a one-way switch that forces sensitive admin mutations
(admin handover, signing-key rotation, WASM upgrade, whitelist-root change, and
delay change) through a `schedule` → wait → `execute` flow with a publicly
observable delay window.

### Entrypoints

| Entrypoint | Auth | Purpose |
| --- | --- | --- |
| `enable_timelock(delay_seconds)` | admin (one-way) | Turn the timelock on (1 hour – 30 days). |
| `timelock_delay()` | — | Current delay, or `None` if disabled. |
| `timelock_schedule(action)` | admin | Queue an action; returns the operation id. |
| `timelock_execute(id)` | admin | Apply a queued operation after its ETA. |
| `timelock_cancel(id)` | admin | Drop a queued operation. |
| `timelock_operation(id)` | — | The queued `TimelockOperation`, or `None`. |
| `timelock_pending()` | — | Ids of all queued operations. |
| `timelock_operation_id(action)` | — | Pre-compute the deterministic operation id. |

### Authorization model

- `enable_timelock`, `timelock_schedule`, `timelock_execute`, and
  `timelock_cancel` are **admin-only**.
- Enabling is **one-way**; the delay can only be changed afterwards by
  scheduling a `TimelockAction::SetTimelockDelay` operation (itself delayed).
- Once enabled, direct `update_admin`, `propose_admin`/`accept_admin`, and
  `upgrade` calls panic with `TimelockRequired`.

### Worked example

```bash
enable_timelock --delay_seconds 172800            # 48h, one-way
timelock_schedule --action '{"SetAdmin":"G..."}'  # returns <id>
timelock_pending                                   # audit the queue
timelock_execute --id <id>                         # apply after the ETA
timelock_cancel --id <id>                          # or abort before the ETA
```

See [docs/timelock.md](docs/timelock.md) for the full architecture and operator
runbook.

## Whitelist (Merkle)

The contract can gate behaviour on a whitelist of addresses without storing the
list on-chain. Only a 32-byte merkle root is published; membership is proven
per-call with a merkle proof.

### Entrypoints

| Entrypoint | Auth | Purpose |
| --- | --- | --- |
| `set_whitelist_root(root)` | admin | Publish or replace the root. |
| `clear_whitelist_root()` | admin | Remove the root, disabling whitelist checks. |
| `get_whitelist_root()` | — | Current root, or `None`. |
| `whitelist_leaf(user)` | — | The leaf hash for an address. |
| `verify_whitelist(user, proof)` | — | `true` if the proof proves membership. |

### Authorization model

- `set_whitelist_root` and `clear_whitelist_root` are **admin-only**.
- `verify_whitelist` and `whitelist_leaf` are permissionless reads.

### Worked example

```bash
set_whitelist_root --root <32-byte-root>
verify_whitelist --user <USER_ADDRESS> --proof '[<sibling-hash>, ...]'
```

See [docs/whitelist-merkle.md](docs/whitelist-merkle.md) for the leaf encoding,
tree layout, and proof ordering.

> **Not yet enforced:** the merkle gate (`require_whitelisted`) exists but is
> **not called** by any mint or transfer entrypoint, so publishing a root
> currently does not restrict who can mint. It is exposed for future
> private-mint phases.

## Batch minting

`mint_wrap_batch` mints up to `MAX_BATCH_SIZE` (100) wraps in one call. Each item
is validated for period, payload version, authorization, and signature.

### Signature modes

`mint_wrap_batch(items, aggregated_signature)` accepts one of two signature
modes:

1. **Individual signatures** (`aggregated_signature = None`): each
   `BatchWrapItem` carries its own Ed25519 `signature` over the canonical
   per-item payload, verified exactly like `mint_wrap`.
2. **Aggregated signature** (`aggregated_signature = Some(sig)`): a single
   signature over the concatenation of all item payloads; every item must use
   the same `payload_version`.

### Authorization model

Each `item.user` must authorise the call (`item.user.require_auth()`), and the
signature(s) must verify against the admin Ed25519 public key. `BatchEmpty` and
`BatchTooLarge` are raised for empty batches or batches larger than 100 items.

### Worked example

```bash
mint_wrap_batch \
  --items '[{"user":"G...","period":202401,"archetype":"arch","data_hash":"...","payload_version":1,"signature":"..."}]' \
  --aggregated_signature 'null'
```

See [docs/signing-payload.md](docs/signing-payload.md) for the canonical payload
layout that both single and batch signatures sign.

## Features not yet enforced

The following capabilities exist in the contract but are **not yet wired into the
mint path**. They are listed so the README does not overstate behaviour:

- **Whitelist gating** — `set_whitelist_root` / `verify_whitelist` and the
  internal `require_whitelisted` gate exist, but no mint or transfer entrypoint
  calls them. Publishing a root does not yet restrict who can mint.
- **Fee collection** — the storage-accounting fee model (`current_fee`,
  `set_fee_params`, `fee_params`) is computed on-chain, but `mint_wrap` does not
  charge it. `transfer_wrap` charges a separate, fixed `set_transfer_fee`
  amount unrelated to the storage fee.
- **Staking discounts** — `get_discounted_fee` computes a discount from a user's
  stake, but mints do not apply it, so staking affects priority numbers only,
  not the fees a user actually pays.

## Oracle hash verification

`verify_with_oracle` performs a read-only cross-contract call to the supplied
oracle address. A compatible oracle exposes this ABI:

```text
verify_data_hash(data_hash: BytesN<32>) -> bool
```

The hash is forwarded unchanged. The oracle returns `true` when its
decentralized verification process recognizes the hash and `false` when it
does not. Contract invocation failures, a missing method, and incompatible
return values propagate as call errors; they are never converted to `false`.

The caller supplies the oracle address, so a `true` response is only as
trustworthy as that selected oracle. Applications should use a vetted oracle
contract ID from their own configuration. This method does not mutate wrap
records and does not replace the local `verify_data` comparison.

## Event schemas

### Mint event
### CLI examples

Placeholder variables:

- `<CONTRACT_ID>` — deployed contract address (e.g. `C...`)
- `<USER_ADDRESS>` — Stellar account address (e.g. `G...`)
- `<PERIOD>` — period encoded as `YYYYMM` (e.g. `202401`)
- `<DATA_HEX>` — hex-encoded raw data bytes

#### `get_wrap`

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_wrap \
  --user <USER_ADDRESS> \
  --period <PERIOD>
```

Returns `Option<WrapRecord>` — either the record (see [WrapRecord](#wraprecord)) or `null`.

#### `get_latest_wrap`

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  -- \
  get_latest_wrap \
  --user <USER_ADDRESS>
```

Returns `Option<WrapRecord>` — same shape as `get_wrap`, or `null`.

#### `balance_of`

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  -- \
  balance_of \
  --user <USER_ADDRESS>
```

Returns an integer count of wraps for the user (e.g. `42`).

#### `verify_data`

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  -- \
  verify_data \
  --user <USER_ADDRESS> \
  --period <PERIOD> \
  --data <DATA_HEX>
```

Returns `true` if `sha256(data)` matches the stored `data_hash`, otherwise `false`.

## Security model

Mint signatures are verified over a canonical payload that binds the request to:

- a domain separator (`stellar-wrap-v1`)
- the deploying contract instance address
- the target user address
- the period (`YYYYMM`)
- the archetype symbol
- the data hash

The payload is constructed by concatenating the XDR-encoded fields in the order above. Off-chain signers should use the same byte layout when creating signatures:

1. encode the domain separator as raw bytes
2. append the XDR encoding of the contract address
3. append the XDR encoding of the user address
4. append the XDR encoding of the period as `u64`
5. append the XDR encoding of the archetype symbol
6. append the XDR encoding of the 32-byte data hash

This ensures that a signature for one contract instance cannot be replayed against another deployment with the same admin key.

## Event schema

Successful wrap mints emit one event:

- **Topic 0**: `mint` (`Symbol`)
- **Topic 1**: `user` (`Address`) - The wallet address that received the wrap
- **Topic 2**: `period` (`u64`) - The period in `YYYYMM` format (e.g., `202401`)
- **Data**: `archetype` (`Symbol`) - The wrap archetype identifier

**Example values:**
- Topic 0: `mint`
- Topic 1: `GD5...` (32-byte Stellar address)
- Topic 2: `202401`
- Data: `arch` (or any short symbol)

**Properties relevant to indexers:**
- The event is emitted only after signature verification and storage writes succeed
- Duplicate `(user, period)` mints are rejected, so one event equals one successful new wrap
- `period` is always a validated `YYYYMM` value (year: 2024-2100, month: 01-12)

### Admin update event

Successful admin rotations emit one event:

- **Topic 0**: `admin` (`Symbol`)
- **Topic 1**: `updated` (`Symbol`)
- **Data**: `(old_admin, new_admin)` (`Address`, `Address`) — previous admin and newly assigned admin

**Example values:**
- Topic 0: `admin`
- Topic 1: `updated`
- Data: `(GOLDADMIN..., GNEWADMIN...)`

**Properties relevant to indexers:**
- The event is emitted only after the current admin authorizes the call and storage is updated
- Indexers can track admin rotations without polling `get_admin(e)`, but should still verify the live admin via that query when enforcing privileged flows

### Revoke event

Successful revocations emit one event:

- **Topic 0**: `revoke` (`Symbol`)
- **Topic 1**: `user` (`Address`)
- **Topic 2**: `period` (`u64`)
- **Data**: `reason_hash` (`BytesN<32>`) — the SHA-256 of an off-chain reason, or a zero hash when omitted

See [docs/revoke-policy.md](docs/revoke-policy.md) for the operational policy.

## Important note for indexers

**⚠️ Do not infer state from events alone.** Use contract queries to verify wrap existence:
- `get_wrap(e, user, period)` to retrieve full wrap record
- `balance_of(e, user)` to get total wrap count for a user

## Leaderboard decision

Issue `#68` is implemented as an off-chain leaderboard strategy.

## Tech Stack

- **Language:** Rust
- **Smart Contract Framework:** Soroban SDK v21.7.1
- **Build Tool:** Cargo
- **Target:** WebAssembly (WASM) for Soroban runtime
- **Testing:** Soroban SDK testutils

> **Note:** Dependency versions are pinned exactly (`=21.7.1`) in `Cargo.toml`. For reproducible builds, always build against the committed `Cargo.lock` (run `cargo build --locked` / `cargo test --locked`) rather than letting Cargo re-resolve versions.

---

Reasoning:

- Soroban storage does not support efficient range scans for ranking
- maintaining an on-chain sorted top-N list would add write amplification and higher gas costs to every mint
- indexers already need mint events for analytics, so leaderboard aggregation fits the existing data flow

Recommended aggregation rule:

1. index every `mint` event
2. group by topic 1 (`user`)
3. count events per user
4. sort descending by count to produce the leaderboard

## Testnet deployment walkthrough

### Prerequisites

**Required tools:**
- Rust and Cargo (for building)
- Stellar CLI (`stellar`) - [installation guide](https://developers.stellar.org/docs/soroban/install)
- Make (optional, for using the Makefile)

**Required accounts:**
- Deployer account with XLM on testnet (for paying deployment fees)
- Admin address (public Stellar address that will control the contract)
- Ed25519 signing key (private key used to sign mint payloads)

**⚠️ Security note:** The admin address and Ed25519 signing key are separate:
- **Admin address**: Public Stellar address stored on-chain for authorization
- **Ed25519 signing key**: Private key used to sign mint payloads (never stored on-chain)
- Keep the Ed25519 private key secure - it can authorize unlimited mints

### Step 1: Build the contract

```bash
# Using Make
make build

# Or using cargo directly
cargo build --release --target wasm32-unknown-unknown
```

This produces the WASM file at `target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm`.

### Step 2: Deploy to testnet

Set your deployer secret key as an environment variable:

```bash
export STELLAR_DEPLOYER_SECRET="S..."
```

Deploy the contract:

```bash
# Using Make
make deploy-testnet

# Or using stellar CLI directly
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm \
  --network testnet \
  --source "$STELLAR_DEPLOYER_SECRET"
```

Save the contract ID output - you'll need it for initialization.

### Step 3: Initialize the contract

You need:
- `CONTRACT_ID`: From step 2
- `ADMIN_ADDRESS`: Your admin Stellar address (public)
- `ADMIN_PUBKEY`: The 32-byte public key of your Ed25519 signing key

To get your Ed25519 public key from your private signing key:

```bash
# If you have the private key in hex format
# This is a placeholder - use your actual Ed25519 key generation tool
# The public key is 32 bytes
```

Initialize the contract:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source "$STELLAR_DEPLOYER_SECRET" \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --admin_pubkey <ADMIN_PUBKEY_HEX>
```

### Step 4: Mint your first wrap

You need to sign a payload with your Ed25519 signing key. The payload binds all
the `mint_wrap` arguments in this exact order:

- `user` — address who will receive the wrap (authorizes the call)
- `period` — in `YYYYMM` format (e.g. `202401` for January 2024)
- `archetype` — symbol classifying the wrap (e.g. `arch`)
- `data_hash` — SHA-256 of your wrap data
- `payload_version` — must be `1` (current payload version)
- `signature` — Ed25519 signature over the canonical payload

Example using a signing script (you'll need to implement this based on your Ed25519 library):

```bash
# 1. Prepare your data and hash it
echo '{"score":100,"level":"gold"}' > data.json
DATA_HASH=$(sha256sum data.json | cut -d' ' -f1)

# 2. Sign the payload with your Ed25519 private key
# (Use your preferred Ed25519 signing tool)
SIGNATURE=$(sign-payload \
  --contract <CONTRACT_ID> \
  --user <USER_ADDRESS> \
  --period 202401 \
  --archetype "arch" \
  --data_hash $DATA_HASH \
  --payload_version 1 \
  --private-key <ED25519_PRIVATE_KEY>)

# 3. Mint the wrap
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <USER_ADDRESS_SECRET> \
  -- mint_wrap \
  --user <USER_ADDRESS> \
  --period 202401 \
  --archetype "arch" \
  --data_hash $DATA_HASH \
  --payload_version 1 \
  --signature $SIGNATURE
```

### Step 5: Verify the mint

Query the contract to verify the wrap was minted:

```bash
stellar contract read \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_wrap \
  --user <USER_ADDRESS> \
  --period 202401
```

## 💿 Upgrade runbook

This runbook covers the end-to-end process of upgrading a deployed contract to
a new WASM version: building, uploading, capturing the WASM hash, invoking the
admin-authorized upgrade, and validating the result.

### How it works

Upgrading a Soroban contract replaces its executable code while **preserving all
storage** (wrap records, admin config, migration state, etc.). No data is lost
during the upgrade.

The contract exposes an `upgrade(new_wasm_hash)` function that:
1. Verifies the contract has been initialized (`NotInitialized` otherwise).
2. Requires authorization from the **admin address** (`Unauthorized` otherwise).
3. Emits an `upgrade` audit event containing the requested WASM hash.
4. Calls `e.deployer().update_current_contract_wasm(new_wasm_hash)` to replace
   the code.

The upgrade function is defined in `src/admin.rs`. The contract code is
implemented in `src/lib.rs`.

> **Storage is preserved.** Any changes to the storage layout must be shipped as
> a numbered migration via `migrate(version)` — see the [Upgrade compatibility]
> section below.

### Prerequisites

- **Stellar CLI** (`stellar`) — [installation guide](https://developers.stellar.org/docs/soroban/install)
- **Admin secret key** for the deployed contract (the same address passed as
  `admin` to `initialize()`)
- **Deployer account** with XLM to cover the upload and invocation fees
- The upgrade runbook assumes **testnet**; for mainnet replace
  `--network testnet` with `--network mainnet` throughout.

### Step 1 — Build the new WASM

```bash
make build
# or: cargo build --release --target wasm32-unknown-unknown
```

The WASM artifact is at:
`target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm`

> **Reproducible builds:** Always build against the committed `Cargo.lock`
> (`cargo build --locked`) to ensure the WASM hash matches across environments.
> The Dockerfile provides a fully isolated build:
> ```bash
> make docker-build
> ```

### Step 2 — Upload the WASM and capture the hash

Upload the new WASM to the network. The CLI returns the **WASM hash** (a 32-byte
hex-encoded SHA-256 of the WASM blob):

```bash
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/stellar_wrap_contract.wasm \
  --network testnet \
  --source <ADMIN_SECRET_KEY>
```

**Save the returned WASM hash.** It will be passed as `new_wasm_hash` to the
`upgrade()` function in the next step.

Alternatively, using the Makefile:

```bash
export STELLAR_DEPLOYER_SECRET="S..."   # or the admin secret
export CONTRACT_ID="<EXISTING_CONTRACT_ID>"
make deploy-testnet
```

The Makefile target prints the WASM hash to stdout. Capture it from the output.

### Step 3 — Invoke the upgrade

Call the contract's `upgrade` function with the captured WASM hash:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <ADMIN_SECRET_KEY> \
  -- \
  upgrade \
  --new_wasm_hash <WASM_HASH_HEX>
```

**Authorization:** The `--source` account must match the admin address stored in
the contract. If it does not, the invocation panics with `Unauthorized` (code 3).

**Failure mode — wrong WASM hash:** If the hash does not correspond to a WASM
blob previously uploaded on the same network, Soroban rejects the upgrade with
a host error. The contract state is **not modified** — storage remains intact.

### Step 4 — Verify the upgrade

#### Smoke check 1 — Confirm health

The health endpoint should still report the contract as initialized:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <ADMIN_SECRET_KEY> \
  -- \
  health
```

Expected output:
```json
{"initialized": true, "has_admin": true, "has_signing_key": true}
```

#### Smoke check 2 — Verify storage is preserved

Existing wrap records must be readable after the upgrade:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- \
  get_wrap \
  --user <USER_ADDRESS> \
  --period <PERIOD>
```

If records existed before the upgrade, they should still be returned. If no
records exist (fresh contract), this returns `null`.

#### Smoke check 3 — Run migrations (if needed)

If the new code introduces a storage migration, call `migrate` immediately
after the upgrade, in the same transaction batch if possible:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source <ADMIN_SECRET_KEY> \
  -- \
  migrate \
  --version <NEXT_VERSION>
```

Verify the migration was applied:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- \
  migration_version
```

Expected output: `NEXT_VERSION` (or whatever version was passed to `migrate`).

> If `migrate` is called twice with the same version, it panics with
> `MigrationAlreadyApplied` (code 7) — see [ERRORS.md](./ERRORS.md).

#### Smoke check 4 — Mint a test wrap

Mint a new wrap to confirm the upgraded code handles write operations correctly:

```bash
# Follow the minting instructions in the testnet deployment walkthrough above
```

### Failure modes

| Symptom | Likely cause | Resolution |
|---------|-------------|------------|
| `Error(Contract, #2)` — `NotInitialized` | Contract has not been `initialize()`'d | Call `initialize(admin, admin_pubkey)` first |
| `Error(Contract, #3)` — `Unauthorized` | `--source` is not the admin address | Use the correct admin secret key |
| `HostError: ...wasm hash...` | WASM hash does not match any uploaded blob | Re-upload the WASM and verify the hash |
| `Error(Contract, #7)` — `MigrationAlreadyApplied` | `migrate` called twice with same version | Check `migration_version()` first; this is not a real error |
| Contract behaves the same as before | Storage is preserved — expected behavior. Check the event log for the `upgrade` audit event | Confirm the upgrade event was emitted: `stellar contract event --id <CONTRACT_ID>` |
| Unexpected storage behaviour | The new code changed a `DataKey` variant or record shape without a migration | Add a migration step and re-upgrade |

### Upgrade compatibility

An upgrade replaces contract code while keeping storage, so any change to the
storage layout must ship as a numbered migration:

- `DataKey::MigrationVersion` stores the highest migration version applied (`0` before any migration).
- `migrate(version)` is admin-only and only accepts a version greater than the stored one, so a
  migration can never run twice — a replay panics with `MigrationAlreadyApplied` (code 7).
- Additive changes (new `DataKey` variants, new methods) need no migration; changing or removing
  the shape of an existing key does, and the new code must bump the migration version.
- Call `migrate` in the same transaction batch as the upgrade, and verify with `migration_version()`.

### Key security considerations

1. **The upgrade function is admin-only.** If the admin keypair is compromised,
   an attacker can replace the contract WASM. Consider a time-lock or multi-sig
   admin for production deployments.
2. **Storage is never wiped.** Sensitive data stored by a previous version
   remains accessible after upgrade. Ensure the new code handles all existing
   `DataKey` variants gracefully.
3. **Audit trail.** Every upgrade emits an `upgrade` event with the new WASM
   hash. Indexers and monitoring tools should watch for unexpected upgrade
   events. The event topic is `upgrade` with data being the new WASM hash.
4. **Rollback.** To revert an upgrade, build and upload the previous WASM, then
   invoke `upgrade` with the old WASM hash. Storage is preserved across
   rollbacks as well.
## Documentation

- [Canonical signed payload encoding](docs/signing-payload.md) — exact field order, XDR encoding rules, and test vectors required by backend signing services (issue #213)
- [Admin rotation procedure](docs/admin-rotation.md) — safe procedure for rotating the admin address and signing pubkey, including verification, event monitoring, and rollback plan
- [Timelock controller](docs/timelock.md) — architecture and operator runbook for the admin timelock
- [Off-chain whitelisting via Merkle proofs](docs/whitelist-merkle.md) — leaf encoding, tree layout, and proof verification
- [Bridge architecture](docs/bridge-architecture.md) — cross-chain bridge workflow and components
- [Revoke policy](docs/revoke-policy.md) — operational policy for `revoke_wrap`
- [Using `verify_data`](docs/verify-data.md) — off-chain JSON integrity checks

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.94.1 with the
`wasm32-unknown-unknown` target), so local, Docker, and CI builds match. With
`rustup` installed, the correct toolchain is selected automatically.

### Front-end dApp

The [`frontend/`](frontend/) directory contains a React dApp for connecting
Freighter, reading a deployed contract, looking up wallet wraps, and submitting
signed `mint_wrap` transactions. See [`frontend/README.md`](frontend/README.md)
for configuration, architecture, security boundaries, and verification
commands.

Run the test suite with:
## Local Development Quickstart

### Prerequisites

- **Rust** – install via [rustup](https://rustup.rs/). The project targets a recent stable toolchain.
- **wasm32 target** – add the WebAssembly compilation target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Stellar CLI** (recommended) – install from the [Stellar soroban-cli releases](https://github.com/stellar/stellar-cli/releases) or via `cargo`:
  ```bash
  cargo install stellar-cli
  ```
  Alternatively, install the legacy **Soroban CLI**:
  ```bash
  cargo install soroban-cli
  ```

### Common commands

| Action | Command |
|---|---|
| Format | `cargo fmt` |
| Format check (CI) | `cargo fmt --check` or `make fmt-check` |
| Lint | `cargo clippy -- -D warnings` or `make lint` |
| Test | `cargo test` or `make test` |
| Fuzz `mint_wrap` | `make fuzz FUZZ_SECONDS=30` |
| Release build (WASM) | `cargo build --release --target wasm32-unknown-unknown` or `make build` |
| Deploy to testnet | `make deploy-testnet` |
| Docker reproducible build | `make docker-build` or `docker build -t stellar-wrap-contract .` |

See the `Makefile` for the full list of targets (`make help`).

### Fuzzing `mint_wrap`

This repo ships a [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) target that
stresses `mint_wrap` with adversarial periods, hashes, and signatures
(`fuzz/fuzz_targets/fuzz_mint_wrap.rs`).

Prerequisites:

```bash
rustup install nightly
rustup component add rust-src --toolchain nightly
cargo install --locked cargo-fuzz
```

Build / run (ThreadSanitizer + `build-std` is required on macOS):

```bash
make fuzz-build
make fuzz FUZZ_SECONDS=30
# equivalent:
cargo +nightly fuzz run --sanitizer=thread --build-std fuzz_mint_wrap -- -max_total_time=30
```

Invariants checked by the harness:

- Invalid periods never persist a wrap or change balances
- Rogue signatures never mint
- A valid admin signature + valid period mints exactly once
- Reminting the same `(user, period)` always fails without changing balance

### Troubleshooting

**"target `wasm32-unknown-unknown` not installed"**
```bash
rustup target add wasm32-unknown-unknown
```

Build the WASM artifact with:

```bash
cargo build --release --target wasm32-unknown-unknown
```
**SDK / toolchain mismatch errors** (e.g. `package \`soroban-sdk\` cannot be built because it requires a different Rust version`)

The Soroban SDK often tracks Rust nightly or a specific stable release. If you see version conflicts:
- Verify your Rust version matches what the lockfile expects:
  ```bash
  rustup show
  rustup update stable
  ```
- If the SDK pins a nightly, install and use it:
  ```bash
  rustup install nightly-YYYY-MM-DD
  rustup target add wasm32-unknown-unknown --toolchain nightly-YYYY-MM-DD
  cargo +nightly-YYYY-MM-DD build --release --target wasm32-unknown-unknown
  ```
- Clean stale artifacts before switching toolchains:
  ```bash
  cargo clean
  ```

**WASM build fails with link errors**
Ensure `wasm32-unknown-unknown` is the active target and no host-specific native dependencies leak in. The `Dockerfile` provides a fully isolated environment for reproducible WASM builds.
## Mainnet deployment

Before deploying to mainnet, review the release checklist in [MAINNET_RELEASE_CHECKLIST.md](MAINNET_RELEASE_CHECKLIST.md). It covers tests, optimized builds, release artifact hash verification, signer backup, initialization, and rollback guidance.
### Gas Analysis

The contract includes gas analysis tests that measure CPU instructions and memory usage
of mint operations. These tests always run assertions on resource bounds, but detailed
budget tables are suppressed during normal test runs to keep CI output clean.

To run tests with full gas budget reporting:

```bash
make test-gas-report
# or
SOROBAN_GAS_REPORT=1 cargo test -- --nocapture
```

> **Note:** The Soroban test framework automatically creates snapshot files under
> `test_snapshots/` during test execution. These are already in `.gitignore` and
> can be cleaned up with `make clean-snapshots`.

## DAO Governance Module

The contract includes a DAO governance module for updating the contract's admin address via community/on-chain proposals.

### Workflow

1. **Create Proposal:** Call `create_admin_proposal(proposer, proposed_admin, duration_seconds)`. Generates a proposal in `Active` status.
2. **Cast Votes:** Accounts vote via `vote_admin_proposal(voter, proposal_id, support)`. Double voting is prevented.
3. **Execute Proposal:** After `duration_seconds` elapses, call `execute_admin_proposal(proposal_id)`. If `votes_for > votes_against`, the contract admin updates to `proposed_admin`.
4. **Cancel Proposal:** Proposer or current admin can cancel active proposals via `cancel_admin_proposal(caller, proposal_id)`.

