# Stellar Wrap Contract

Soroban contract for storing non-transferable Stellar Wrap records by wallet and reporting successful wrap mints through events.

## Contract layout

The contract is split into focused modules:

- `src/lib.rs`: contract type and module wiring
- `src/admin.rs`: initialization and admin updates
- `src/mint.rs`: period validation, signature verification, wrap minting, event emission
- `src/queries.rs`: read-only queries and metadata
- `src/errors.rs`: contract error codes
- `src/storage_types.rs`: storage keys and persisted record types

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

## Storage keys

- `DataKey::Admin`
- `DataKey::AdminPubKey`
- `DataKey::Wrap(Address, u64)`
- `DataKey::WrapCount(Address)`
- `DataKey::LatestPeriod(Address)`

## Public interface

### Write methods

- `initialize(e: Env, admin: Address, admin_pubkey: BytesN<32>)`
- `update_admin(e: Env, new_admin: Address)`
- `mint_wrap(e: Env, user: Address, period: u64, archetype: Symbol, data_hash: BytesN<32>, signature: BytesN<64>)`

### Read methods

- `get_wrap(e: Env, user: Address, period: u64) -> Option<WrapRecord>`
- `balance_of(e: Env, user: Address) -> i128`
- `verify_data(e: Env, user: Address, period: u64, data: Bytes) -> bool`
- `get_latest_wrap(e: Env, user: Address) -> Option<WrapRecord>`
- `get_admin(e: Env) -> Option<Address>`
- `name(e: Env) -> String`
- `symbol(e: Env) -> String`
- `decimals(e: Env) -> u32`

## Event schema

Successful wrap mints emit one event:

- Topic 0: `mint`
- Topic 1: `user` (`Address`)
- Topic 2: `period` (`u64`, `YYYYMM`)
- Data: `archetype` (`Symbol`)

Properties relevant to indexers:

- the event is emitted only after signature verification and storage writes succeed
- duplicate `(user, period)` mints are rejected, so one event equals one successful new wrap
- `period` is always a validated `YYYYMM` value

## Leaderboard decision

Issue `#68` is implemented as an off-chain leaderboard strategy.

Reasoning:

- Soroban storage does not support efficient range scans for ranking
- maintaining an on-chain sorted top-N list would add write amplification and higher gas costs to every mint
- indexers already need mint events for analytics, so leaderboard aggregation fits the existing data flow

Recommended aggregation rule:

1. index every `mint` event
2. group by topic 1 (`user`)
3. count events per user
4. sort descending by count to produce the leaderboard

## Development

Run the test suite with:

```bash
cargo test
```
