import { loadConfig } from './config';
import { IndexerDB } from './db';
import { SorobanFetcher } from './fetcher';
import { processEventBatch, createEmptyState, persistStateToDB } from './processor';
import { backfillEvents } from './backfill';
import { reconcile } from './reconciler';
import type { DerivedState } from './types';

/**
 * Per-counter tolerance thresholds for reconciliation.
 *
 * `total_wraps` is the canonical, monotonic counter and must match on-chain
 * state exactly — any deviation is a genuine divergence (a bug), not noise.
 * Other counters are allowed a small slack to absorb indexer lag.
 */
const RECONCILE_TOLERANCE: Record<string, number> = {
  total_wraps: 0,
};

/**
 * Counters whose drift is expected while the indexer is still catching up to
 * the chain head. Drift here is reported as lag, not as a failure.
 */
const LAG_TOLERANT_COUNTERS = new Set<string>(['total_unwraps', 'total_transfers']);

interface CounterDrift {
  counter: string;
  indexed: number;
  onchain: number;
  delta: number;
  tolerance: number;
  kind: 'divergence' | 'lag';
}

interface ReconciliationRun {
  ran_at: string;
  contract_id: string;
  indexed_ledger: number;
  chain_head_ledger: number;
  is_consistent: boolean;
  has_divergence: boolean;
  drifts: CounterDrift[];
}

/**
 * Compare indexed vs on-chain counters, applying per-counter tolerances and
 * classifying each drift as either indexer lag or genuine divergence.
 */
function analyzeDrift(
  report: Awaited<ReturnType<typeof reconcile>>,
  indexedLedger: number,
  chainHeadLedger: number,
): CounterDrift[] {
  const drifts: CounterDrift[] = [];
  const indexed = report.indexed as unknown as Record<string, number>;
  const onchain = report.onchain as unknown as Record<string, number>;

  for (const counter of Object.keys(onchain)) {
    const indexedValue = indexed[counter] ?? 0;
    const onchainValue = onchain[counter] ?? 0;
    const delta = onchainValue - indexedValue;
    const tolerance = RECONCILE_TOLERANCE[counter] ?? 0;

    if (Math.abs(delta) <= tolerance) {
      continue;
    }

    // A positive delta (chain ahead of index) on a lag-tolerant counter while
    // the indexer has not yet reached the chain head is expected lag, not a bug.
    const isLag =
      delta > 0 &&
      LAG_TOLERANT_COUNTERS.has(counter) &&
      indexedLedger < chainHeadLedger;

    drifts.push({
      counter,
      indexed: indexedValue,
      onchain: onchainValue,
      delta,
      tolerance,
      kind: isLag ? 'lag' : 'divergence',
    });
  }

  return drifts;
}

/**
 * Persist a reconciliation run and its outcome so drift appearing between two
 * runs can be bisected to a ledger range.
 */
function recordReconciliationRun(db: IndexerDB, run: ReconciliationRun): void {
  try {
    db.recordReconciliation({
      ran_at: run.ran_at,
      contract_id: run.contract_id,
      indexed_ledger: run.indexed_ledger,
      chain_head_ledger: run.chain_head_ledger,
      is_consistent: run.is_consistent,
      has_divergence: run.has_divergence,
      drifts: JSON.stringify(run.drifts),
    });
  } catch (err) {
    // Recording is best-effort; never let it mask the reconciliation outcome.
    console.error('Failed to record reconciliation run:', err instanceof Error ? err.message : err);
  }
}

/**
 * Resolve the ledger to resume from on startup.
 *
 * The persisted cursor is authoritative: it is written transactionally with
 * the derived state, so it always reflects the last fully-processed ledger.
 * `START_LEDGER` is only consulted on a genuine first run (no cursor yet).
 *
 * If the persisted cursor is ahead of the chain head the indexer refuses to
 * start rather than spinning on a ledger the chain has not produced.
 */
async function resolveStartLedger(
  db: IndexerDB,
  fetcher: SorobanFetcher,
  contractId: string,
  firstRunDefault: number,
): Promise<number> {
  const cursor = db.getCursor(`cursor:${contractId}`);

  if (!cursor) {
    console.log(`No persisted cursor; starting from START_LEDGER=${firstRunDefault}`);
    return firstRunDefault;
  }

  const chainHead = await fetcher.getLatestLedger();
  if (cursor.last_processed_ledger > chainHead) {
    throw new Error(
      `Persisted cursor (ledger ${cursor.last_processed_ledger}) is ahead of the ` +
        `chain head (ledger ${chainHead}). Refusing to start; the database may ` +
        `belong to a different network or the chain may have been reset.`,
    );
  }

  console.log(
    `Resuming from persisted cursor at ledger ${cursor.last_processed_ledger} ` +
      `(chain head ${chainHead})`,
  );
  return cursor.last_processed_ledger + 1;
}

async function main(): Promise<void> {
  const config = loadConfig();
  const db = await IndexerDB.create(config.db_path);
  const fetcher = new SorobanFetcher({
    rpcUrl: config.rpc_url,
    contractId: config.contract_id,
    eventPageSize: config.event_page_size,
  });

  console.log('Soroban-RPC Indexer Pipeline');
  console.log('============================');
  console.log(`Contract: ${config.contract_id}`);
  console.log(`RPC URL:  ${config.rpc_url}`);
  console.log(`DB Path:  ${config.db_path}`);

  // ── Reconcile-only mode ────────────────────────────────────────────
  if (config.reconcile_only) {
    console.log('\nRunning reconciliation...');
    const report = await reconcile(db, fetcher, config.contract_id);

    const cursor = db.getCursor(`cursor:${config.contract_id}`);
    const indexedLedger = cursor ? cursor.last_processed_ledger : 0;
    const chainHeadLedger = report.onchain.latest_ledger ?? indexedLedger;

    const drifts = analyzeDrift(report, indexedLedger, chainHeadLedger);
    const divergences = drifts.filter((d) => d.kind === 'divergence');
    const lags = drifts.filter((d) => d.kind === 'lag');
    const hasDivergence = divergences.length > 0;

    console.log(`\nReconciliation ${hasDivergence ? 'FAILED' : 'PASSED'}`);
    console.log(`Ledger range: ${indexedLedger}-${chainHeadLedger}`);

    if (lags.length > 0) {
      console.log('Indexer lag (not a failure):');
      for (const d of lags) {
        console.log(`  - ${d.counter}: indexed=${d.indexed} onchain=${d.onchain} delta=${d.delta}`);
      }
    }

    if (divergences.length > 0) {
      console.log('Divergences:');
      for (const d of divergences) {
        console.log(
          `  - ${d.counter}: indexed=${d.indexed} onchain=${d.onchain} ` +
          `delta=${d.delta} tolerance=${d.tolerance}`,
        );
      }
    }

    if (report.mismatches.length > 0) {
      console.log('Mismatches:');
      for (const m of report.mismatches) {
        console.log(`  - ${m}`);
      }
    }
    console.log(`Indexed wraps: ${report.indexed.total_wraps}`);
    console.log(`On-chain wraps: ${report.onchain.total_wraps}`);

    recordReconciliationRun(db, {
      ran_at: new Date().toISOString(),
      contract_id: config.contract_id,
      indexed_ledger: indexedLedger,
      chain_head_ledger: chainHeadLedger,
      is_consistent: !hasDivergence,
      has_divergence: hasDivergence,
      drifts,
    });

    db.close();

    // Exit non-zero on genuine divergence so this is usable as a scheduled check.
    if (hasDivergence) {
      process.exit(1);
    }
    return;
  }

  // ── Backfill mode ──────────────────────────────────────────────────
  if (config.backfill) {
    console.log('\nStarting historical backfill...');
    await backfillEvents({
      fetcher,
      db,
      contractId: config.contract_id,
      startLedger: config.start_ledger,
      onProgress: (processed, ledger) => {
        if (processed % 500 === 0) {
          console.log(`  Processed ${processed} events (ledger ${ledger})`);
        }
      },
    });
    console.log('Backfill complete.');

    // Run reconciliation after backfill
    console.log('\nRunning post-backfill reconciliation...');
    const report = await reconcile(db, fetcher, config.contract_id);

    const cursor = db.getCursor(`cursor:${config.contract_id}`);
    const indexedLedger = cursor ? cursor.last_processed_ledger : 0;
    const chainHeadLedger = report.onchain.latest_ledger ?? indexedLedger;

    const drifts = analyzeDrift(report, indexedLedger, chainHeadLedger);
    const hasDivergence = drifts.some((d) => d.kind === 'divergence');

    console.log(`Reconciliation: ${hasDivergence ? 'FAILED' : 'PASSED'}`);
    for (const d of drifts) {
      console.log(
        `  - [${d.kind}] ${d.counter}: indexed=${d.indexed} onchain=${d.onchain} delta=${d.delta}`,
      );
    }
    for (const m of report.mismatches) {
      console.log(`  - ${m}`);
    }

    recordReconciliationRun(db, {
      ran_at: new Date().toISOString(),
      contract_id: config.contract_id,
      indexed_ledger: indexedLedger,
      chain_head_ledger: chainHeadLedger,
      is_consistent: !hasDivergence,
      has_divergence: hasDivergence,
      drifts,
    });

    db.close();
    return;
  }

  // ── Live indexing mode ─────────────────────────────────────────────
  console.log('\nStarting live indexing...');

  // Resume from the persisted cursor when present; START_LEDGER is a
  // first-run default only. Refuse to start if the cursor is ahead of the
  // chain head instead of spinning on a ledger the chain has not produced.
  let startLedger: number;
  try {
    startLedger = await resolveStartLedger(db, fetcher, config.contract_id, config.start_ledger);
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    db.close();
    process.exit(1);
  }

  // Initialize state from DB or create empty
  let state: DerivedState;
  const persistedState = db.getDerivedState(config.contract_id);
  if (persistedState) {
    state = persistedState;
  } else {
    state = createEmptyState(config.contract_id);
  }

  // The cursor and the derived state are written together in a single
  // transaction by persistStateToDB, so a restart can never observe a cursor
  // that disagrees with the data derived from it.
  let lastProcessedLedger = startLedger - 1;

  while (true) {
    const batch = await fetcher.fetchEvents(startLedger);

    if (batch.events.length === 0) {
      await new Promise((resolve) => setTimeout(resolve, config.poll_interval_ms));
      continue;
    }

    const result = processEventBatch(state, batch.events);
    state = result.state;

    const batchLedger = batch.events.reduce(
      (max, event) => Math.max(max, event.ledger),
      lastProcessedLedger,
    );

    persistStateToDB(db, state, config.contract_id, batchLedger, batch.events);
    lastProcessedLedger = batchLedger;
    startLedger = batchLedger + 1;

    console.log(`Processed ${batch.events.length} events up to ledger ${batchLedger}`);
  }
}

main().catch((err) => {
  console.error('Fatal error:', err instanceof Error ? err.message : err);
  process.exit(1);
});
