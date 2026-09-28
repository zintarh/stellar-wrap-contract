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
 * Conditions worth paging a human about on a deployed contract. Each maps to a
 * single alert so operators can route and silence them independently.
 */
export type AlertCondition =
  | 'contract_paused'
  | 'admin_change'
  | 'governance_proposal_created'
  | 'governance_proposal_executed'
  | 'timelock_action_scheduled'
  | 'bridge_chain_disabled'
  | 'reconciliation_drift';

/**
 * A single alert emitted from reconciler output. `expected` distinguishes a
 * privileged action that was scheduled/announced from one nobody asked for —
 * an unexpected admin change is the signal that actually matters.
 */
export interface ContractAlert {
  condition: AlertCondition;
  contract_id: string;
  ledger_seq: number;
  /** True when the action was scheduled/announced ahead of time. */
  expected: boolean;
  /** Human-readable summary routed to the alert destination. */
  message: string;
  /** Structured context for the alert payload. */
  details: Record<string, unknown>;
  timestamp: string;
}

/**
 * Destination for alerts. Implementations route to a human-visible channel
 * (webhook/Slack/PagerDuty) rather than a log line.
 */
export interface AlertSink {
  send(alert: ContractAlert): Promise<void>;
}

/**
 * Privileged actions that were scheduled/announced ahead of time. Anything not
 * present here is treated as unexpected and escalated.
 */
export interface ExpectedActions {
  /** Admin pubkeys whose change was scheduled. */
  admin_changes?: string[];
  /** Governance proposal ids expected to be created. */
  proposal_creations?: number[];
  /** Governance proposal ids expected to be executed. */
  proposal_executions?: number[];
  /** Timelock action ids expected to be scheduled. */
  timelock_actions?: number[];
  /** Bridge chain ids expected to be disabled. */
  bridge_disables?: number[];
}

/**
 * Operational health snapshot for the indexer. This is the primary signal an
 * operator or monitor polls: `lag` is the gap between chain head and the last
 * processed ledger, and `behind` is the alertable form of it.
 */
export interface IndexerHealth {
  /** Chain head ledger observed at snapshot time. */
  chain_head_ledger: number;
  /** Highest ledger the indexer has processed. */
  last_processed_ledger: number;
  /** Gap between chain head and last processed ledger (>= 0). */
  lag: number;
  /** True when lag exceeds the configured alert threshold. */
  behind: boolean;
  /** Threshold (in ledgers) above which `behind` becomes true. */
  lag_threshold: number;
  /** Ledgers processed per second since the indexer started. */
  processing_rate: number;
  /** Cumulative decode failures observed. */
  decode_failures: number;
  /** Cumulative retry attempts observed. */
  retry_count: number;
  /** Outcome of the most recent reconciliation run, if any. */
  last_reconciliation: ReconciliationRun | null;
  /** True when the last reconciliation reported genuine divergence. */
  last_reconciliation_ok: boolean;
  timestamp: string;
}

/**
 * Mutable counters the indexer updates as it runs. Kept separate from the DB so
 * health can be reported without a schema change.
 */
export interface IndexerMetrics {
  /** Highest ledger the indexer has processed. */
  last_processed_ledger: number;
  /** Cumulative decode failures. */
  decode_failures: number;
  /** Cumulative retry attempts. */
  retry_count: number;
  /** Wall-clock ms when the indexer started, for rate calculation. */
  started_at_ms: number;
}

/**
 * Create a fresh metrics record for a newly started indexer.
 */
export function createIndexerMetrics(startedAtMs: number = Date.now()): IndexerMetrics {
  return {
    last_processed_ledger: 0,
    decode_failures: 0,
    retry_count: 0,
    started_at_ms: startedAtMs,
  };
}

/**
 * Build the operational health snapshot from live metrics, the chain head, and
 * the last recorded reconciliation run. This is the single source of truth for
 * "is the indexer keeping up?" and is safe to poll from a monitor.
 */
export function buildIndexerHealth(
  metrics: IndexerMetrics,
  chainHeadLedger: number,
  lagThreshold: number,
  lastReconciliation: ReconciliationRun | null,
  nowMs: number = Date.now(),
): IndexerHealth {
  const lag = Math.max(0, chainHeadLedger - metrics.last_processed_ledger);
  const elapsedSeconds = Math.max(0, (nowMs - metrics.started_at_ms) / 1000);
  const processingRate = elapsedSeconds > 0 ? metrics.last_processed_ledger / elapsedSeconds : 0;
  return {
    chain_head_ledger: chainHeadLedger,
    last_processed_ledger: metrics.last_processed_ledger,
    lag,
    behind: lag > lagThreshold,
    lag_threshold: lagThreshold,
    processing_rate: processingRate,
    decode_failures: metrics.decode_failures,
    retry_count: metrics.retry_count,
    last_reconciliation: lastReconciliation,
    last_reconciliation_ok: lastReconciliation ? !lastReconciliation.genuine_divergence : true,
    timestamp: new Date(nowMs).toISOString(),
  };
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

/**
 * Derive alerts from a reconciliation run. Reuses the reconciler's own output
 * (the report and the recorded run) rather than building a second view of
 * chain state. Drift is only alerted when it is genuine divergence, not when
 * the indexer is merely lagging behind chain head.
 */
export function alertsFromReconciliationRun(run: ReconciliationRun): ContractAlert[] {
  if (!run.genuine_divergence) return [];
  return [
    {
      condition: 'reconciliation_drift',
      contract_id: run.contract_id,
      ledger_seq: run.ledger_seq,
      expected: false,
      message: `Reconciliation drift on ${run.contract_id} at ledger ${run.ledger_seq}: ${run.affected_counters.join(', ')}`,
      details: {
        affected_counters: run.affected_counters,
        indexed_ledger_seq: run.indexed_ledger_seq,
        chain_head_ledger: run.chain_head_ledger,
      },
      timestamp: run.timestamp,
    },
  ];
}

/**
 * Derive an alert when the indexer is behind by more than the threshold. This
 * turns "behind by more than N ledgers" into an alertable condition instead of
 * something an operator has to discover by inspection.
 */
export function alertsFromIndexerHealth(health: IndexerHealth): ContractAlert[] {
  if (!health.behind) return [];
  return [
    {
      condition: 'reconciliation_drift',
      contract_id: '',
      ledger_seq: health.last_processed_ledger,
      expected: false,
      message: `Indexer is behind by ${health.lag} ledgers (threshold ${health.lag_threshold})`,
      details: {
        lag: health.lag,
        lag_threshold: health.lag_threshold,
        chain_head_ledger: health.chain_head_ledger,
        last_processed_ledger: health.last_processed_ledger,
        processing_rate: health.processing_rate,
      },
      timestamp: health.timestamp,
    },
  ];
}
