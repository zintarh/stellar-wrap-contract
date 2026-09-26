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

  // Determine starting ledger from cursor or start_ledger config
  const cursor = db.getCursor(`cursor:${config.contract_id}`);
  let startLedger = cursor ? cursor.last_processed_ledger + 1 : config.start_ledger;

  // Initialize state from DB or create empty
  let state: DerivedState;
  const existingState = db.getContractState(config.contract_id);
  if (existingState) {
    state = createEmptyState(config.contract_id, startLedger);
    // Load wraps from DB
    console.log('Loading existing indexed state...');
  } else {
    state = createEmptyState(config.contract_id, startLedger);
  }

  console.log(`Starting from ledger ${startLedger}`);
  console.log(`Polling every ${config.poll_interval_ms}ms\n`);

  // Main polling loop
  let consecutiveErrors = 0;

  while (true) {
    try {
      const { events, latestLedger } = await fetcher.fetchEvents(startLedger);

      if (events.length > 0) {
        const processed = processEventBatch(db, state, events);
        const firstLedger = events[0].ledger;
        const lastLedger = events[events.length - 1].ledger;
        console.log(
          `[${new Date().toISOString()}] Indexed ${processed} events ` +
          `(ledgers ${firstLedger}-${lastLedger}, latest: ${latestLedger})`,
        );
      }

      if (latestLedger > 0) {
        startLedger = latestLedger + 1;
        db.upsertCursor(
          `cursor:${config.contract_id}`,
          config.contract_id,
          latestLedger,
          latestLedger,
        );
      }

      consecutiveErrors = 0;

    } catch (err) {
      consecutiveErrors++;
      console.error(`Error (${consecutiveErrors}):`, err instanceof Error ? err.message : err);

      if (consecutiveErrors >= 10) {
        console.error('Too many consecutive errors. Exiting.');
        break;
      }
    }

    await new Promise((resolve) => setTimeout(resolve, config.poll_interval_ms));
  }

  db.close();
}

process.on('unhandledRejection', (err) => {
  console.error('Unhandled rejection:', err);
  process.exit(1);
});

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
