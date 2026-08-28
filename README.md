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

## Public interface

### Write methods

- `initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>)`
- `update_admin(e: Env, new_admin: Address)`
- `mint_wrap(e: Env, user: Address, period: u64, archetype: Symbol, data_hash: BytesN<32>, payload_version: u32, signature: BytesN<64>)`
- `mint_wrap_batch(e: Env, items: Vec<BatchWrapItem>, aggregated_signature: Option<BytesN<64>>)`
- `revoke_wrap(e: Env, user: Address, period: u64, reason_hash: BytesN<32>)`
- `transfer_wrap(e: Env, from: Address, to: Address, period: u64)`
- `expire_wrap(e: Env, user: Address, period: u64)`
- `migrate(e: Env, version: u32)`
- `upgrade(e: Env, new_wasm_hash: BytesN<32>)`

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

### Read methods

- `get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord>`  
  Returns the wrap record for the specified user and period. Safe to call before initialization — returns `None` if the contract has not been initialized or if no wrap exists for the given user and period.
- `balance_of(e: Env, user: Address) -> i128`
- `verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool`
- `verify_with_oracle(e: Env, oracle: Address, data_hash: BytesN<32>) -> bool`
- `get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord>`
- `get_admin(e: Env) -> Option<Address>`
- `health(e: Env) -> ContractHealth`
- `name(e: Env) -> String`
- `symbol(e: Env) -> String`
- `decimals(e: Env) -> u32`
- `migration_version(e: Env) -> u32`

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

## Emergency Pause Model

The contract includes an emergency pause mechanism (`pause()` / `unpause()`) managed by the admin address. When the contract is paused, state-mutating wrap and token/staking operations are immediately blocked to halt state mutations during security incident investigations.

### Entrypoint Pause Audit Table

| Entrypoint | Category | Blocked When Paused? | Rationale |
|---|---|---|---|
| `mint_wrap` | State Mutating | **Yes** | Block new wrap minting during incidents |
| `mint_wrap_batch` | State Mutating | **Yes** | Block batch wrap minting during incidents |
| `transfer_wrap` | State Mutating | **Yes** | Block wrap ownership transfer during incidents |
| `backfill_wrap_periods` | State Mutating | **Yes** | Block wrap period index modifications during incidents |
| `transition_wrap_state` | State Mutating | **Yes** | Block wrap lifecycle state transitions during incidents |
| `expire_wrap` | State Mutating | **Yes** | Block wrap state expiration mutations during incidents |
| `stake` | State Mutating | **Yes** | Block new token staking operations during incidents |
| `unstake` | State Mutating | **Yes** | Block unstake operations during incidents |
| `withdraw_stake` | State Mutating | **Yes** | Block stake withdrawals during incidents |
| `bridge_wrap_out` | State Mutating | **Yes** | Block outbound cross-chain wrap bridges during incidents |
| `bridge_wrap_in` | State Mutating | **Yes** | Block inbound cross-chain wrap bridge fulfillments during incidents |
| `unpause` | Admin Control | **No** | Must remain callable by Admin to restore normal operation |
| Admin / Governance (`pause`, `update_admin`, `propose_admin`, `accept_admin`, `cancel_proposed_admin`, `set_transfer_fee`, `migrate`, `upgrade`, `set_name`, `set_symbol`, `set_expiration_duration`, `set_fee_params`, `set_whitelist_root`, `clear_whitelist_root`, `enable_timelock`, `timelock_*`, `set_bridge_relayer`, `set_chain_status`, `create_admin_proposal`, `vote_admin_proposal`, `execute_admin_proposal`, `cancel_admin_proposal`, `set_stake_config`) | Admin / Governance | **No** | Emergency administration, governance voting, and operational controls must remain available to allow incident response and governance resolution while user wrap mutations are frozen |
| `burn_wrap`, `revoke_wrap`, `opt_out`, `opt_in`, `set_alias_hash` | User / Admin Operations | **No** | User identity, opt-out preferences, and administrative revocations |
| `extend_ttl`, `renew_all_ttls` | Maintenance | **No** | Pure storage TTL maintenance; extends storage lifetime without mutating wrap ownership or token balances |
| Read-only views (`is_paused`, `get_wrap`, `get_mint_timestamp`, `get_last_updated`, `total_wrap_count`, `verify_data`, `verify_with_oracle`, `get_latest_wrap`, `get_wraps`, `get_all_wraps_for_user`, `get_admin`, `get_admin_pubkey`, `version`, `has_wrap`, `get_transfer_fee`, `health`, etc.) | Read-Only Views | **No** | View methods are non-mutating queries and remain safe to call while paused |

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

Revoke functionality is not implemented in this contract. Wraps are non-transferable and permanent once minted.

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

