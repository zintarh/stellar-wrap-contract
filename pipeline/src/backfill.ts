import { SorobanFetcher } from './fetcher';
import { isTransientRpcError } from './retry';
import { IndexerDB } from './db';
import { processEventBatch, createEmptyState, persistStateToDB } from './processor';
import { alertsFromRetentionGaps } from './reconciler';
import type { DerivedState, RetentionGap } from './types';

export interface BackfillOptions {
  fetcher: SorobanFetcher;
  db: IndexerDB;
  contractId: string;
  startLedger: number;
  endLedger?: number;
  onProgress?: (processed: number, currentLedger: number) => void;
}

/**
 * Check whether `startLedger` has already been pruned by Soroban-RPC. Events in
 * a pruned range can never be fetched again, so the gap is recorded (and
 * alerted) and the caller must resume from the returned `resumeLedger`, the
 * oldest ledger RPC still retains. Without this the fetch fails or skips the
 * range with no record that it happened.
 */
export async function detectRetentionGap(
  fetcher: SorobanFetcher,
  db: IndexerDB,
  contractId: string,
  startLedger: number,
): Promise<{ resumeLedger: number; gap: RetentionGap | null }> {
  const oldestLedger = await fetcher.getOldestLedger();
  if (oldestLedger === 0 || startLedger >= oldestLedger) {
    return { resumeLedger: startLedger, gap: null };
  }

  const gap: RetentionGap = {
    contract_id: contractId,
    from_ledger: startLedger,
    to_ledger: oldestLedger - 1,
    detected_at: new Date().toISOString(),
  };
  db.recordRetentionGap(gap);
  for (const alert of alertsFromRetentionGaps([gap])) {
    console.error(`ALERT [${alert.condition}] ${alert.message}`);
  }
  return { resumeLedger: oldestLedger, gap };
}

/**
 * Backfill historical events for a contract from startLedger to endLedger
 * (or latest ledger if endLedger is not specified).
 *
 * This processes events in pages and incrementally builds the derived state.
 */
export async function backfillEvents(opts: BackfillOptions): Promise<{
  processed: number;
  finalLedger: number;
  state: DerivedState;
}> {
  const { fetcher, db, contractId, startLedger, onProgress } = opts;
  const endLedger = opts.endLedger ?? (await fetcher.getLatestLedger());
  const retention = await detectRetentionGap(fetcher, db, contractId, startLedger);

  let currentLedger = retention.resumeLedger;
  let totalProcessed = 0;
  let state = createEmptyState(contractId, startLedger);

  console.log(`Backfilling events from ledger ${startLedger} to ${endLedger}...`);

  while (currentLedger <= endLedger) {
    try {
      const { events, latestLedger } = await fetcher.fetchEvents(currentLedger);

      if (events.length > 0) {
        const result = processEventBatch(state, events);
        state = result.state;
        persistStateToDB(db, state, contractId, latestLedger, events);
        totalProcessed += result.processed;
      }

      const nextLedger = latestLedger > 0 ? latestLedger + 1 : currentLedger + 100;

      if (onProgress) {
        onProgress(totalProcessed, currentLedger);
      }

      currentLedger = nextLedger;

    } catch (err) {
      if (!isTransientRpcError(err)) {
        // Permanent error (e.g. invalid contract id) will fail forever — don't retry.
        throw err;
      }
      console.error(`Error at ledger ${currentLedger}:`, err);
      // Transient outage: keep the batch alive so we catch up when RPC recovers.
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }
  }

  console.log(`Backfill complete. Processed ${totalProcessed} events.`);

  return {
    processed: totalProcessed,
    finalLedger: endLedger,
    state,
  };
}
