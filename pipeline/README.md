# Soroban-RPC indexer pipeline

TypeScript indexer that follows Stellar Wrap contract events and storage over Soroban-RPC, derives wrap/user/contract state, and persists it to a local SQLite database (`sql.js`). Downstream services (read API, renewal bot) consume that store.

## Architecture

```
Soroban-RPC
    │
    ▼
┌─────────┐    XDR / ScVal     ┌─────────┐    typed events     ┌───────────┐
│ fetcher │ ─────────────────► │ decoder │ ──────────────────► │ processor │
└─────────┘                    └─────────┘                     └─────┬─────┘
                                                                     │
                                                          derived state
                                                                     │
                                                                     ▼
                                                               ┌─────────┐
                                                               │  store  │  (SQLite)
                                                               └────┬────┘
                                          ┌─────────────────────────┼─────────────────────────┐
                                          ▼                         ▼                         ▼
                                    read API / bot            reconciler                 operators
                                                              (on-chain vs indexed)
```

| Module | Role |
| --- | --- |
| `src/fetcher.ts` | Talks to Soroban-RPC (`getEvents`, `getLedgerEntries`, latest ledger). Retries transient RPC failures via `src/retry.ts`. |
| `src/decoder.ts` | Decodes ledger keys, storage values, and event topics/data from XDR/`ScVal` into typed structures. |
| `src/processor.ts` | Classifies events, applies them to in-memory derived state, and persists batches transactionally with the ledger cursor. |
| `src/db.ts` | SQLite schema and accessors: events, wraps, user/contract state, cursor, applied-event idempotency, reconciliation runs. |
| `src/backfill.ts` | Historical catch-up from `START_LEDGER` (or a chosen range) to chain head. |
| `src/reconciler.ts` | Compares indexed counters/fields to live on-chain storage; classifies lag vs genuine divergence; exposes health/metrics helpers. |
| `src/index.ts` | CLI entry: live poll, `--backfill`, or `--reconcile-only`. |

Event shapes the contract emits are documented in the repo root [README event catalogue](../README.md#event-catalogue). Keep decoder/processor changes aligned with that table.

## Setup

Requires Node.js 20+.

```bash
cd pipeline
npm ci
# create pipeline/.env (see env vars below)
```

### Environment variables

Loaded from `pipeline/.env` (see `src/config.ts`).

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `CONTRACT_ID` | **yes** | — | Wrap contract C-address to index |
| `RPC_URL` | no | `https://soroban-testnet.stellar.org` | Soroban-RPC endpoint |
| `DB_PATH` | no | `pipeline/indexer.db` | SQLite file path |
| `POLL_INTERVAL_MS` | no | `5000` | Sleep between empty live polls |
| `EVENT_PAGE_SIZE` | no | `100` | Max events per RPC page |
| `START_LEDGER` | no | `1` | First-run start only; ignored once a cursor exists |
| `ALERT_WEBHOOK_URL` | no | _(empty)_ | Optional alert sink URL |
| `ALERT_MIN_SEVERITY` | no | `warning` | `info` \| `warning` \| `critical` |
| `EXPECTED_ADMIN` | no | _(empty)_ | Expected admin for alert context |

Example `.env` for testnet:

```env
CONTRACT_ID=C...your_deployed_wrap_contract...
RPC_URL=https://soroban-testnet.stellar.org
START_LEDGER=1
DB_PATH=./indexer.db
```

For a local quickstart node (see root README / `docker-compose.yml`):

```env
RPC_URL=http://localhost:8000/rpc
```

### Scripts

| Command | What it does |
| --- | --- |
| `npm start` | Live indexing loop (resume from cursor) |
| `npm run backfill` | Historical backfill, then reconciliation |
| `npm run reconcile` | One-shot reconcile-only check (exit `1` on divergence) |
| `npm test` | Unit tests (`vitest run`) |
| `npm run test:watch` | Watch mode |
| `npm run typecheck` / `npm run lint` | `tsc --noEmit` |
| `npm run build` | Compile to JS |

CI runs typecheck, tests with coverage, and build on `pipeline/**` changes (`.github/workflows/pipeline.yml`).

## Running tests

```bash
cd pipeline
npm ci
npm test
```

Integration checks that hit a live network are gated:

```bash
RUN_INTEGRATION=1 npm test
```

## Running against testnet

1. Set `CONTRACT_ID` to the deployed Wrap contract and keep the default testnet `RPC_URL` (or point at your RPC).
2. Optional: set `START_LEDGER` to the ledger near contract deploy for a faster first backfill.
3. Catch up history, then keep the live indexer running:

```bash
npm run backfill
npm start
```

4. Periodically (or via cron) verify consistency:

```bash
npm run reconcile
```

`START_LEDGER` is only used when no cursor is stored. After the first successful commit, restarts resume from `ledger_cursor` and ignore `START_LEDGER`.

## Operations

### Backfill

`npm run backfill` pages events from `START_LEDGER` (or the configured start) through the current chain head, applies them through the processor, and persists derived state. On completion it runs reconciliation and prints lag vs divergence.

Use backfill for:

- First-time indexing of an already-deployed contract
- Rebuilding after an intentional schema re-index (see schema policy in `src/config.ts` / `src/db.ts`)

### Gap handling and restarts

- **Cursor + state are committed together** (`commitLedger` / `persistStateToDB`), so a crash cannot leave a cursor ahead of applied data.
- **Idempotent apply**: `applied_events` records event IDs; replays and overlapping pages do not double-count wraps.
- **Transient RPC errors**: fetcher/backfill use bounded exponential backoff with jitter (`src/retry.ts`). Permanent errors (e.g. bad contract id) fail fast.
- **Cursor ahead of chain head**: startup refuses to run (wrong network or chain reset) instead of spinning forever.
- **Empty live polls**: sleep `POLL_INTERVAL_MS` and retry from the same start ledger.

If you must re-index from scratch after a non-migratable schema change, delete (or move aside) the DB file and re-run backfill with an appropriate `START_LEDGER`.

### Reconciliation

`npm run reconcile` loads indexed contract/wrap state from SQLite and compares it to current on-chain storage via the fetcher + decoder.

- `total_wraps` is zero-tolerance: any mismatch is a failure.
- Drift while the indexer is still behind chain head on lag-tolerant counters is reported as **lag**, not failure.
- Genuine divergence exits with code `1` (suitable for a scheduled check).
- Each run can be recorded in `reconciliation_runs` so later drift can be bisected by ledger range.

### Metrics and health

`src/reconciler.ts` defines operator-facing snapshots (not a Prometheus scrape endpoint in-process):

- **`IndexerMetrics`**: last processed ledger, decode failures, retry count, start time
- **`IndexerHealth`**: chain-head lag, `behind` vs lag threshold, processing rate, last reconciliation outcome
- **`db.getStats(contractId)`**: event/wrap/user counts and last indexed ledger for quick inspection

Alert helpers (`alertsFromReconciliationRun`, `alertsFromIndexerHealth`) turn genuine divergence and excessive lag into structured `ContractAlert`s for an `AlertSink` (e.g. webhook). Wire `ALERT_WEBHOOK_URL` / severity when deploying alerting.

## Layout

```
pipeline/
├── package.json
├── tsconfig.json
├── vitest.config.ts
├── src/
│   ├── index.ts          # entry (live / backfill / reconcile)
│   ├── config.ts         # env + flags
│   ├── fetcher.ts        # Soroban-RPC client
│   ├── decoder.ts        # XDR → typed keys/values/events
│   ├── processor.ts      # classify, derive, persist
│   ├── db.ts             # SQLite store
│   ├── backfill.ts
│   ├── reconciler.ts
│   ├── retry.ts
│   ├── types.ts
│   └── __tests__/
└── README.md             # this file
```

Closes [#857](https://github.com/mexes20/stellar-wrap-contract/issues/857).
