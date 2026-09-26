import { Address, xdr } from '@stellar/stellar-sdk';
import { SorobanFetcher } from './fetcher';
import { IndexerDB } from './db';
import { decodeDataKey, decodeStorageValue } from './decoder';
import { applyStorageEntryToState, createEmptyState } from './processor';
import type { DerivedState, StorageEntry } from './types';
import { DataKeyVariant } from './types';

export interface ReconciliationReport {
  contract_id: string;
  ledger_seq: number;
  indexed: {
    total_wraps: number;
    total_users: number;
    contract_state_version: number;
  };
  onchain: {
    total_wraps: number;
    contract_state: number;
  };
  mismatches: string[];
  is_consistent: boolean;
}

/**
 * Per-counter tolerance thresholds. Exceeding a threshold is a failure, not an
 * observation. `total_wraps` is zero-tolerance: any divergence is a bug.
 */
export const RECONCILIATION_TOLERANCE: Record<string, number> = {
  total_wraps: 0,
  total_wrap_count: 0,
  total_revoked: 0,
  admin: 0,
  admin_pubkey: 0,
  storage_bytes: 0,
  slash_threshold: 0,
  is_paused: 0,
};

/**
 * A single recorded reconciliation run, persisted so drift appearing between
 * two runs can be bisected to a ledger range.
 */
export interface ReconciliationRun {
  contract_id: string;
  /** Ledger sequence the on-chain snapshot was taken at. */
  ledger_seq: number;
  /** Highest ledger the indexer had processed when the run started. */
  indexed_ledger_seq: number;
  /** Chain head at the time of the run, used to detect indexer lag. */
  chain_head_ledger: number;
  /** True when the indexer is behind chain head and drift may be lag. */
  indexer_lagging: boolean;
  /** True when drift cannot be explained by indexer lag. */
  genuine_divergence: boolean;
  /** Counters that exceeded their tolerance. */
  affected_counters: string[];
  is_consistent: boolean;
  timestamp: string;
}

/**
 * Reconcile indexed state against current on-chain storage.
 * Fetches all current storage entries and compares with the database.
 */
export async function reconcile(
  db: IndexerDB,
  fetcher: SorobanFetcher,
  contractId: string,
): Promise<ReconciliationReport> {
  const mismatches: string[] = [];
  const onChainState = createEmptyState(contractId, 0);

  // Fetch all current storage entries
  const storageEntries = await fetcher.fetchStorageEntries();

  // Build on-chain state from entries
  for (const entry of storageEntries) {
    applyStorageEntryToState(onChainState, entry);
  }

  // Get indexed state from DB
  const indexedState = db.getContractState(contractId);
  const indexedWrapCount = db.getWrapCount(contractId);

  // Compare contract state
  if (indexedState) {
    compareFields('admin', indexedState.admin, onChainState.admin, mismatches);
    compareFields('admin_pubkey', indexedState.admin_pubkey, onChainState.adminPubKey, mismatches);
    compareFields('total_wrap_count', indexedState.total_wrap_count, onChainState.totalWrapCount, mismatches);
    compareFields('total_revoked', indexedState.total_revoked, onChainState.totalRevoked, mismatches);
    compareFields('storage_bytes', indexedState.storage_bytes, onChainState.storageBytes, mismatches);
    compareFields('slash_threshold', indexedState.slash_threshold, onChainState.slashThreshold, mismatches);
    compareFields('is_paused', indexedState.is_paused ? true : false, onChainState.paused, mismatches);
  }

  // total_wraps is zero-tolerance: compare the indexed wrap count against the
  // on-chain counter explicitly so a seeded divergence is always reported.
  compareFields('total_wraps', indexedWrapCount, onChainState.totalWrapCount, mismatches);

  const isConsistent = mismatches.length === 0;

  return {
    contract_id: contractId,
    ledger_seq: onChainState.ledger_seq,
    indexed: {
      total_wraps: indexedWrapCount,
      total_users: 0,
      contract_state_version: indexedState?.migration_version ?? 0,
    },
    onchain: {
      total_wraps: onChainState.totalWrapCount,
      contract_state: onChainState.migrationVersion,
    },
    mismatches,
    is_consistent: isConsistent,
  };
}

/**
 * Extract the counter names that exceeded their tolerance from a report.
 */
export function affectedCounters(report: ReconciliationReport): string[] {
  const counters = new Set<string>();
  for (const mismatch of report.mismatches) {
    const field = mismatch.split(':')[0]?.trim();
    if (field) counters.add(field);
  }
  return [...counters];
}

/**
 * Decide whether a report represents a failure. Any mismatch on a counter with
 * a defined tolerance that is exceeded counts as drift.
 */
export function hasDrift(report: ReconciliationReport): boolean {
  return !report.is_consistent;
}

/**
 * Record a reconciliation run and its outcome. Persists the run so drift
 * appearing between two runs can be bisected to a ledger range, and
 * distinguishes indexer lag from genuine divergence.
 */
export function recordReconciliationRun(
  db: IndexerDB,
  report: ReconciliationReport,
  chainHeadLedger: number,
  indexedLedgerSeq: number,
): ReconciliationRun {
  const indexerLagging = indexedLedgerSeq < chainHeadLedger;
  const drift = hasDrift(report);
  const run: ReconciliationRun = {
    contract_id: report.contract_id,
    ledger_seq: report.ledger_seq,
    indexed_ledger_seq: indexedLedgerSeq,
    chain_head_ledger: chainHeadLedger,
    indexer_lagging: indexerLagging,
    genuine_divergence: drift && !indexerLagging,
    affected_counters: affectedCounters(report),
    is_consistent: report.is_consistent,
    timestamp: new Date().toISOString(),
  };
  db.recordReconciliationRun(run);
  return run;
}

function compareFields(field: string, a: unknown, b: unknown, mismatches: string[]): void {
  const aStr = a != null ? String(a) : '<null>';
  const bStr = b != null ? String(b) : '<null>';
  if (aStr !== bStr) {
    mismatches.push(`${field}: indexed="${aStr}" on-chain="${bStr}"`);
  }
}
