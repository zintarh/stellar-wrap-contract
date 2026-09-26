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
      message: `Reconciliation drift on ${run.contract_id}: ${run.affected_counters.join(', ') || 'unknown counters'}`,
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
 * Derive alerts from a reconciliation report. Reuses the report's own
 * mismatches to detect privileged-state changes (pause, admin) rather than
 * re-reading chain state.
 */
export function alertsFromReconciliationReport(
  report: ReconciliationReport,
  expected: ExpectedActions = {},
): ContractAlert[] {
  const alerts: ContractAlert[] = [];
  const timestamp = new Date().toISOString();

  for (const mismatch of report.mismatches) {
    const field = mismatch.split(':')[0]?.trim();
    if (field === 'is_paused') {
      alerts.push({
        condition: 'contract_paused',
        contract_id: report.contract_id,
        ledger_seq: report.ledger_seq,
        expected: false,
        message: `Contract ${report.contract_id} pause state changed: ${mismatch}`,
        details: { mismatch },
        timestamp,
      });
    } else if (field === 'admin' || field === 'admin_pubkey') {
      const onChain = mismatch.split('on-chain=')[1]?.replace(/"/g, '') ?? '';
      const isExpected = (expected.admin_changes ?? []).includes(onChain);
      alerts.push({
        condition: 'admin_change',
        contract_id: report.contract_id,
        ledger_seq: report.ledger_seq,
        expected: isExpected,
        message: isExpected
          ? `Scheduled admin change on ${report.contract_id}: ${mismatch}`
          : `UNEXPECTED admin change on ${report.contract_id}: ${mismatch}`,
        details: { mismatch, on_chain_admin: onChain },
        timestamp,
      });
    }
  }

  return alerts;
}

/**
 * Derive alerts for governance and bridge privileged actions. `expected`
 * distinguishes scheduled/announced actions from ones nobody asked for.
 */
export function alertsFromPrivilegedActions(
  contractId: string,
  ledgerSeq: number,
  actions: {
    proposal_created?: number[];
    proposal_executed?: number[];
    timelock_scheduled?: number[];
    bridge_disabled?: number[];
  },
  expected: ExpectedActions = {},
): ContractAlert[] {
  const alerts: ContractAlert[] = [];
  const timestamp = new Date().toISOString();

  for (const id of actions.proposal_created ?? []) {
    const isExpected = (expected.proposal_creations ?? []).includes(id);
    alerts.push({
      condition: 'governance_proposal_created',
      contract_id: contractId,
      ledger_seq: ledgerSeq,
      expected: isExpected,
      message: isExpected
        ? `Governance proposal ${id} created on ${contractId}`
        : `UNEXPECTED governance proposal ${id} created on ${contractId}`,
      details: { proposal_id: id },
      timestamp,
    });
  }

  for (const id of actions.proposal_executed ?? []) {
    const isExpected = (expected.proposal_executions ?? []).includes(id);
    alerts.push({
      condition: 'governance_proposal_executed',
      contract_id: contractId,
      ledger_seq: ledgerSeq,
      expected: isExpected,
      message: isExpected
        ? `Governance proposal ${id} executed on ${contractId}`
        : `UNEXPECTED governance proposal ${id} executed on ${contractId}`,
      details: { proposal_id: id },
      timestamp,
    });
  }

  for (const id of actions.timelock_scheduled ?? []) {
    const isExpected = (expected.timelock_actions ?? []).includes(id);
    alerts.push({
      condition: 'timelock_action_scheduled',
      contract_id: contractId,
      ledger_seq: ledgerSeq,
      expected: isExpected,
      message: isExpected
        ? `Timelock action ${id} scheduled on ${contractId}`
        : `UNEXPECTED timelock action ${id} scheduled on ${contractId}`,
      details: { action_id: id },
      timestamp,
    });
  }

  for (const id of actions.bridge_disabled ?? []) {
    const isExpected = (expected.bridge_disables ?? []).includes(id);
    alerts.push({
      condition: 'bridge_chain_disabled',
      contract_id: contractId,
      ledger_seq: ledgerSeq,
      expected: isExpected,
      message: isExpected
        ? `Bridge chain ${id} disabled on ${contractId}`
        : `UNEXPECTED bridge chain ${id} disabled on ${contractId}`,
      details: { chain_id: id },
      timestamp,
    });
  }

  return alerts;
}

/**
 * Route alerts to a human-visible destination. Unexpected privileged actions
 * are always sent; expected ones are sent too but flagged so the sink can
 * route them to a lower-severity channel.
 */
export async function dispatchAlerts(sink: AlertSink, alerts: ContractAlert[]): Promise<void> {
  for (const alert of alerts) {
    await sink.send(alert);
  }
}

function compareFields(field: string, a: unknown, b: unknown, mismatches: string[]): void {
  const aStr = a != null ? String(a) : '<null>';
  const bStr = b != null ? String(b) : '<null>';
  if (aStr !== bStr) {
    mismatches.push(`${field}: indexed="${aStr}" on-chain="${bStr}"`);
  }
}
