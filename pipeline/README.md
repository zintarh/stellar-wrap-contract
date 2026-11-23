# Stellar Wrap Indexer Pipeline

A standalone Node.js process that tails the Soroban RPC event stream for a deployed Stellar Wrap contract, builds a local SQLite database of derived state, and checks that database against live on-chain storage.

## Table of contents

- [What the indexer does](#what-the-indexer-does)
- [Prerequisites](#prerequisites)
- [Installation](#installation)
- [Configuration](#configuration)
- [Running the indexer](#running-the-indexer)
  - [Normal indexing mode](#normal-indexing-mode)
  - [Backfill mode](#backfill-mode)
  - [Reconcile-only mode](#reconcile-only-mode)
- [Database schema](#database-schema)
- [Querying derived state](#querying-derived-state)
- [Reconciliation reference](#reconciliation-reference)
- [Schema migrations](#schema-migrations)
- [Troubleshooting](#troubleshooting)

---

## What the indexer does

The indexer is an event-driven pipeline that builds and maintains a local SQLite view of the on-chain Stellar Wrap contract. It is useful when the Soroban RPC endpoint is too slow or expensive to query directly for every consumer request.

At a high level the indexer:

1. Polls `getEvents` on the Soroban RPC endpoint, starting from a configurable ledger.
2. Decodes each contract event (`mint`, `revoke`, `transition`, `pause`, `admin_update`, `upgrade`, `slash_*`) into a typed representation.
3. Derives in-memory state from those events: per-user wrap records, aggregate counts, contract config, lifecycle FSM states.
4. Persists the derived state and a durable ledger cursor to SQLite in a single transaction, so a crash never produces a cursor that disagrees with the data it points to.
5. Resumes automatically from the persisted cursor on restart; `START_LEDGER` is only consulted on a genuine first run.

The database stores:

- Every raw event, keyed by its ledger/transaction coordinates.
- Every wrap record with its lifecycle state (Draft → Pending → Active → Archived / Cancelled).
- Per-user summaries: wrap count, latest period, all known periods, alias hash, slash count.
- Contract-level state: admin address, admin public key, pause state, migration version, revoke count, storage-byte estimate.
- A ledger cursor that tracks how far the indexer has progressed.
- A history of reconciliation runs.

---

## Prerequisites

- **Node.js** 18 or later
- **npm** (bundled with Node.js)
- A running Soroban RPC endpoint (testnet: `https://soroban-testnet.stellar.org`)
- The deployed contract ID (`C…`)

---

## Installation

```bash
cd pipeline
npm install
```

---

## Configuration

All configuration is read from environment variables. The easiest way to supply them is a `.env` file in the `pipeline/` directory (it is loaded automatically via `dotenv`):

```bash
# pipeline/.env
CONTRACT_ID=C...
RPC_URL=https://soroban-testnet.stellar.org
DB_PATH=./indexer.db
POLL_INTERVAL_MS=5000
EVENT_PAGE_SIZE=100
START_LEDGER=1
```

### Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `CONTRACT_ID` | **Yes** | — | The Soroban contract address (`C…`) to index. The indexer refuses to start without this. |
| `RPC_URL` | No | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint URL. Change to `https://soroban-mainnet.stellar.org` for mainnet or to `http://localhost:8000/rpc` for a local network. |
| `DB_PATH` | No | `<pipeline-dir>/indexer.db` | Path to the SQLite file. An in-memory database is not used; the file is created on first run. Use an absolute path in production to avoid ambiguity. |
| `POLL_INTERVAL_MS` | No | `5000` | How long (in milliseconds) the indexer waits before re-polling when there are no new events. Reduce for lower latency; increase to reduce RPC load. Has no effect in backfill or reconcile-only mode. |
| `EVENT_PAGE_SIZE` | No | `100` | Number of events to request per RPC call. The maximum the Soroban RPC accepts is `100`; reduce if the endpoint returns timeout errors on large pages. |
| `START_LEDGER` | No | `1` | The ledger sequence to start from on a **first run** (no persisted cursor). Once a cursor has been written to the database this value is ignored entirely. To re-index from scratch, delete the database file and set `START_LEDGER` to the contract's deployment ledger. Setting this value on an existing database has no effect. |
| `ALERT_WEBHOOK_URL` | No | _(empty)_ | Webhook URL for alert delivery (Slack, PagerDuty, etc.). No alerts are sent when empty. |
| `ALERT_MIN_SEVERITY` | No | `warning` | Minimum severity level for alert delivery: `info`, `warning`, or `critical`. |
| `EXPECTED_ADMIN` | No | _(empty)_ | Expected admin address. When set, the reconciler flags a mismatch as an unexpected admin change. |

---

## Running the indexer

The pipeline has three modes, selected by command-line flags and npm scripts.

### Normal indexing mode

Starts the indexer and keeps it running indefinitely, tailing new events as ledgers close.

```bash
npm start
```

Or with `ts-node` directly:

```bash
npx ts-node src/index.ts
```

**What it does:**

1. Loads configuration and opens (or creates) the SQLite database.
2. Runs schema migrations if the database is older than the current schema version.
3. Reads the persisted cursor, or falls back to `START_LEDGER` on a first run.
4. Refuses to start if the cursor is ahead of the chain head (indicates a network reset or wrong database).
5. Enters a poll loop: fetches a page of events, processes them, writes derived state and advances the cursor — all in one transaction — then sleeps for `POLL_INTERVAL_MS` if no new events were found.

**When to use:** the standard production configuration. Keep it running as a long-lived process (e.g. under `systemd`, `pm2`, or in a container).

**Cursor behaviour:** the cursor and the derived state are committed atomically. A SIGKILL between two polls leaves the database in the last fully-committed state; the indexer will re-fetch and process any events from the last committed ledger forward on the next start.

---

### Backfill mode

Processes the full history of the contract from `START_LEDGER` (or the persisted cursor, whichever is later) to the current chain head, then exits. A reconciliation run is performed automatically after the backfill completes.

```bash
npm run backfill
```

Or:

```bash
npx ts-node src/index.ts --backfill
```

**What it does:**

1. Fetches events in pages from `startLedger` to the latest ledger, processing each page incrementally.
2. Progress is logged every 500 processed events.
3. Transient RPC errors (rate limits, timeouts, 5xx) are retried with exponential backoff (up to 5 retries, starting at 500 ms, capped at 30 s). Permanent errors (invalid contract ID, JSON-RPC protocol errors) fail immediately.
4. After catching up to the chain head, runs a full reconciliation and prints the result.
5. Exits with code `0` on success, or `1` if the post-backfill reconciliation finds genuine divergence.

**When to use:**

- Initial population of a fresh database for a contract that has been running for some time.
- Recovering from a gap caused by the indexer being offline.
- After deleting and recreating the database to re-index from scratch.

**Re-indexing from scratch:** delete `indexer.db`, set `START_LEDGER` to the contract deployment ledger, and run backfill mode. The cursor is only written once the first page of events is fully processed, so the process is safe to interrupt and re-run.

**Combining with live indexing:** backfill mode exits after reaching the chain head. Switch to normal indexing mode afterwards to continue tailing new ledgers.

---

### Reconcile-only mode

Performs a single reconciliation check between the local database and the live on-chain state, prints the result, and exits.

```bash
npm run reconcile
```

Or:

```bash
npx ts-node src/index.ts --reconcile-only
```

**What it does:**

1. Fetches all current storage entries from the contract via Soroban RPC (`getLedgerEntries`).
2. Decodes each storage entry into a typed value using the same decoder as the event pipeline.
3. Compares on-chain counters and fields against the values in the local database.
4. Classifies each deviation as either **lag** (the indexer has not yet caught up to the chain head, so the chain being ahead is expected) or **divergence** (the indexed value disagrees with on-chain state in a way that cannot be explained by lag — this is a bug).
5. Prints a `PASSED` / `FAILED` summary and records the run in `reconciliation_runs`.
6. Exits with code `0` on pass, `1` on genuine divergence.

**Tolerance policy:**

| Counter | Tolerance | Rationale |
|---|---|---|
| `total_wraps` | `0` | Monotonic; any divergence is a bug. |
| `total_unwraps` | lag-tolerant | The indexer may not have processed recent events yet. |
| `total_transfers` | lag-tolerant | Same as above. |
| All other counters | `0` | Must match exactly. |

**When to use:**

- As a scheduled cron job (e.g. every 5 minutes) to detect silent divergence between the index and the chain.
- After a backfill to verify the result.
- After an upgrade or migration to confirm the database reflects the new on-chain state.
- In a CI pipeline against a local network to catch regressions.

The exit code makes reconcile-only mode composable with monitoring infrastructure: a non-zero exit triggers an alert.

---

## Database schema

The SQLite database lives at `DB_PATH` (default: `pipeline/indexer.db`). It is managed by `sql.js` and written as a binary file. The current schema version is **3**; the version is persisted in a `schema_meta` table and checked on every startup.

### `schema_meta`

Stores schema metadata. Used by the migration system.

| Column | Type | Description |
|---|---|---|
| `key` | TEXT PK | Metadata key (e.g. `schema_version`). |
| `value` | TEXT | Metadata value. |

### `contract_events`

The raw event log. Every decoded event the indexer has seen is stored here exactly once (idempotent on `id`).

| Column | Type | Description |
|---|---|---|
| `id` | TEXT PK | Stable event identity: `{ledger}:{tx_hash}:{event_index}`. |
| `contract_id` | TEXT | The contract address this event belongs to. |
| `event_type` | TEXT | Classified type: `mint`, `revoke`, `transition`, `init`, `pause`, `upgrade`, `admin_update`, `slash_report`, `slash_clear`, `slash_threshold`, or `unknown`. |
| `ledger_seq` | INTEGER | Ledger sequence the event was emitted in. |
| `tx_hash` | TEXT | Transaction hash. |
| `topics_json` | TEXT | JSON-encoded decoded topics array. |
| `data_json` | TEXT | JSON-encoded decoded event data. |
| `failed_call` | INTEGER | `1` if the event came from a failed invocation (these are never processed into derived state). |
| `created_at` | TEXT | Wall-clock insertion time. |

**Indexes:** `(contract_id, ledger_seq)`, `(event_type)`.

### `wrap_records`

One row per unique `(contract_id, user, period)` triple. Upserted on every `mint` event and on reconciliation.

| Column | Type | Description |
|---|---|---|
| `contract_id` | TEXT | Contract address. |
| `user` | TEXT | Stellar address of the wrap holder. |
| `period` | INTEGER | Period in `YYYYMM` format (e.g. `202401`). |
| `timestamp` | INTEGER | On-chain mint timestamp (`u64`); `0` when derived from an event only (not yet filled from storage). |
| `data_hash` | TEXT | Hex-encoded 32-byte SHA-256 of the wrap data; empty string when derived from an event only. |
| `archetype` | TEXT | Wrap archetype symbol. |
| `fsm_state` | INTEGER | Lifecycle FSM state: `1`=Draft, `2`=Pending, `3`=Active, `4`=Archived, `5`=Cancelled. |
| `fsm_updated_at` | INTEGER | Ledger sequence when the FSM state was last updated. |
| `ledger_seq` | INTEGER | Last ledger that updated this row. |
| `tx_hash` | TEXT | Transaction hash of the last update. |
| `created_at` | TEXT | Row creation time. |
| `updated_at` | TEXT | Row last-updated time. |

**Indexes:** `(contract_id, user)`, `(contract_id, period)`.

### `user_state`

One row per unique `(contract_id, user)` pair. Aggregated from all wrap events for that user.

| Column | Type | Description |
|---|---|---|
| `contract_id` | TEXT | Contract address. |
| `user` | TEXT | Stellar address. |
| `wrap_count` | INTEGER | Number of active wraps. |
| `latest_period` | INTEGER | Most recent period minted (`YYYYMM`), or `NULL`. |
| `alias_hash` | TEXT | 32-byte alias hash set by the user via `set_alias_hash`, or `NULL`. |
| `slash_count` | INTEGER | Number of slash reports against this user. |
| `is_slashed` | INTEGER | `1` when `slash_count >= slash_threshold`. |
| `periods_json` | TEXT | JSON array of all known periods for this user, sorted ascending. |
| `ledger_seq` | INTEGER | Last ledger that updated this row. |
| `updated_at` | TEXT | Row last-updated time. |

**Index:** `(contract_id, user)` (primary key).

### `contract_state`

One row per contract. Holds the current decoded contract-level configuration.

| Column | Type | Description |
|---|---|---|
| `contract_id` | TEXT PK | Contract address. |
| `admin` | TEXT | Current admin Stellar address, or `NULL` before initialization. |
| `admin_pubkey` | TEXT | Hex-encoded 32-byte Ed25519 signing key, or `NULL`. |
| `pending_admin` | TEXT | Address proposed via `propose_admin`, or `NULL`. |
| `migration_version` | INTEGER | Highest migration applied on-chain. |
| `is_paused` | INTEGER | `1` when the contract is paused. |
| `total_wrap_count` | INTEGER | Cumulative minted wraps (monotonically increasing). |
| `total_revoked` | INTEGER | Cumulative revoked wraps. |
| `storage_bytes` | INTEGER | Estimated on-chain storage usage (bytes). |
| `slash_threshold` | INTEGER | Reports needed before a user is considered slashed (default `3`). |
| `ledger_seq` | INTEGER | Last ledger that updated this row. |
| `updated_at` | TEXT | Row last-updated time. |

### `ledger_cursor`

Tracks how far the indexer has progressed. The cursor and the derived state it describes are committed in the same SQLite transaction.

| Column | Type | Description |
|---|---|---|
| `id` | TEXT PK | `cursor:{contract_id}`. |
| `contract_id` | TEXT | Contract address. |
| `last_processed_ledger` | INTEGER | Highest ledger fully processed. |
| `last_event_ledger` | INTEGER | Ledger of the most recent event seen (may differ from `last_processed_ledger` when a page contained events from multiple ledgers). |
| `updated_at` | TEXT | Row last-updated time. |

### `applied_events`

Idempotency ledger: records every event that has been applied to derived state. Prevents double-applying events during a replay, backfill, or retry.

| Column | Type | Description |
|---|---|---|
| `event_id` | TEXT PK | Same `{ledger}:{tx_hash}:{event_index}` identity as `contract_events.id`. |
| `contract_id` | TEXT | Contract address. |
| `event_type` | TEXT | Classified event type. |
| `ledger_seq` | INTEGER | Ledger the event was emitted in. |
| `applied_at` | TEXT | Wall-clock application time. |

**Index:** `(contract_id, ledger_seq)`.

### `storage_snapshots`

Point-in-time snapshots of individual storage entries fetched during reconciliation. Useful for debugging divergence: compare two snapshots taken around the time of the anomaly.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | Row ID. |
| `contract_id` | TEXT | Contract address. |
| `ledger_seq` | INTEGER | Ledger the snapshot was taken at. |
| `key_variant` | TEXT | DataKey variant (e.g. `Wrap`, `Admin`, `TotalWrapCount`). |
| `key_json` | TEXT | Full decoded key as JSON. |
| `value_type` | TEXT | Decoded value type (e.g. `address`, `u32`, `wrap_record`). |
| `value_json` | TEXT | Full decoded value as JSON. |
| `durability` | TEXT | `persistent`, `temporary`, or `instance`. |
| `created_at` | TEXT | Snapshot time. |

**Index:** `(contract_id, ledger_seq)`.

### `reconciliation_runs`

History of every reconciliation run.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | Row ID. |
| `contract_id` | TEXT | Contract address. |
| `ran_at` | TEXT | ISO-8601 timestamp of the run. |
| `ledger_seq` | INTEGER | On-chain ledger observed during the run. |
| `indexed_ledger` | INTEGER | Last indexed ledger at the time of the run. |
| `chain_head_ledger` | INTEGER | Chain head at the time of the run. |
| `is_consistent` | INTEGER | `1` when no counters exceeded their tolerance. |
| `has_divergence` | INTEGER | `1` when genuine (non-lag) divergence was found. |
| `drifts` | TEXT | JSON array of affected counter names. |

---

## Querying derived state

Open the database with any SQLite client (e.g. the `sqlite3` CLI or DB Browser for SQLite).

```bash
sqlite3 pipeline/indexer.db
```

### Check indexer progress

```sql
SELECT contract_id, last_processed_ledger, last_event_ledger, updated_at
FROM ledger_cursor;
```

### Active wraps for a user

```sql
SELECT period, archetype, fsm_state, data_hash, ledger_seq
FROM wrap_records
WHERE contract_id = '<CONTRACT_ID>'
  AND user = '<USER_ADDRESS>'
  AND fsm_state = 3   -- 3 = Active
ORDER BY period;
```

### All wraps across all users, most recent first

```sql
SELECT user, period, archetype, fsm_state, ledger_seq
FROM wrap_records
WHERE contract_id = '<CONTRACT_ID>'
ORDER BY ledger_seq DESC;
```

### Users with the most active wraps (leaderboard)

```sql
SELECT user, wrap_count, latest_period
FROM user_state
WHERE contract_id = '<CONTRACT_ID>'
  AND wrap_count > 0
ORDER BY wrap_count DESC
LIMIT 20;
```

### Contract-level counters

```sql
SELECT admin, is_paused, total_wrap_count, total_revoked,
       migration_version, ledger_seq, updated_at
FROM contract_state
WHERE contract_id = '<CONTRACT_ID>';
```

### Recent reconciliation results

```sql
SELECT ran_at, indexed_ledger, chain_head_ledger,
       is_consistent, has_divergence, drifts
FROM reconciliation_runs
ORDER BY id DESC
LIMIT 10;
```

### Events for a specific ledger range

```sql
SELECT event_type, ledger_seq, tx_hash, topics_json, data_json
FROM contract_events
WHERE contract_id = '<CONTRACT_ID>'
  AND ledger_seq BETWEEN 1000000 AND 1001000
ORDER BY ledger_seq;
```

### All mint events for a user

```sql
SELECT ledger_seq, tx_hash, topics_json, data_json
FROM contract_events
WHERE contract_id = '<CONTRACT_ID>'
  AND event_type = 'mint'
  AND data_json LIKE '%<USER_ADDRESS>%'
ORDER BY ledger_seq;
```

### Slashed users

```sql
SELECT user, slash_count, is_slashed, periods_json
FROM user_state
WHERE contract_id = '<CONTRACT_ID>'
  AND is_slashed = 1;
```

---

## Reconciliation reference

Reconciliation compares every indexed counter against a fresh read of the live on-chain storage via `getLedgerEntries`. It is the authoritative answer to "does the index agree with the chain?".

Fields compared: `admin`, `admin_pubkey`, `total_wrap_count`, `total_revoked`, `storage_bytes`, `slash_threshold`, `is_paused`, and `total_wraps` (computed directly from the `wrap_records` row count).

A deviation is classified as:

- **Lag** — the chain is ahead of the index on a lag-tolerant counter (`total_unwraps`, `total_transfers`), and the indexer has not yet caught up to the chain head. This is expected during indexing and does not trigger a failure.
- **Divergence** — the indexed value disagrees in a way that cannot be explained by lag. The reconciler exits with code `1` and records the affected counters in `reconciliation_runs.drifts`.

`total_wraps` has **zero tolerance**: any divergence here is treated as a bug regardless of lag.

---

## Schema migrations

The schema version is stored in `schema_meta` and checked at startup. The indexer refuses to open a database whose schema version is newer than the code supports (`SchemaVersionError`).

Migrations run automatically on startup in the direction of the current version. They are additive and expressed as `CREATE TABLE IF NOT EXISTS` / `ALTER TABLE` statements, so they are safe to run against an existing database.

A change that cannot be expressed as a forward migration (e.g. a destructive reshape of existing data) must bump `SCHEMA_VERSION` in `db.ts`. The indexer will then refuse to start against the old database with a clear error, making re-indexing an explicit operator decision rather than a silent data loss.

**To re-index from scratch:** delete `indexer.db`, set `START_LEDGER` to the contract deployment ledger, and run backfill mode.

---

## Troubleshooting

### `CONTRACT_ID environment variable is required`

Set `CONTRACT_ID` in `pipeline/.env` or as an environment variable before starting:

```bash
export CONTRACT_ID=C...
npm start
```

### Cursor is ahead of the chain head

```
Persisted cursor (ledger N) is ahead of the chain head (ledger M). Refusing to start;
the database may belong to a different network or the chain may have been reset.
```

The database was built against a different network (e.g. testnet, but now pointing at mainnet) or the chain was reset. Delete `indexer.db` and re-run backfill mode, making sure `RPC_URL` and `CONTRACT_ID` match the intended network.

### `SchemaVersionError: Database schema version N is newer than the supported version M`

You are running an older version of the indexer against a database created by a newer version. Either upgrade the indexer code, or restore a database compatible with this version.

### RPC timeouts / rate limit errors

The indexer retries transient failures automatically (up to 5 attempts, exponential backoff with 25% jitter, capped at 30 s). Persistent rate limiting from the default public endpoint can be reduced by:

- Lowering `EVENT_PAGE_SIZE` to `25` or `50`.
- Increasing `POLL_INTERVAL_MS` to `10000`.
- Pointing `RPC_URL` at a dedicated / private RPC node.

### Post-backfill reconciliation fails

Run reconcile-only mode to get a detailed diff:

```bash
npm run reconcile
```

Check the `reconciliation_runs` table to see which counters diverged and at which ledger the divergence was recorded. If `total_wraps` is mismatched, the most common cause is a gap in the event feed; delete the database and re-run backfill with `START_LEDGER` set to the contract deployment ledger.

### Events are being skipped

Events from failed invocations (`failed_call = 1`) are intentionally ignored; they never change on-chain state and must not be applied to derived state. Check `contract_events` to confirm:

```sql
SELECT COUNT(*) FROM contract_events WHERE failed_call = 1;
```

### Running tests

```bash
npm test
```

Tests use an in-memory sql.js database and an injected mock RPC client; no network or file-system access is required.
