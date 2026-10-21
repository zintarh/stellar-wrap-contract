import { Address, xdr } from '@stellar/stellar-sdk';
import { SorobanFetcher } from './fetcher';
import { IndexerDB } from './db';
import { decodeDataKey, decodeStorageValue } from './decoder';
import { applyStorageEntryToState, createEmptyState } from './processor';
import type { DerivedState, StorageEntry, WrapRecord, WrapRow, WrapState } from './types';
import { DataKeyVariant } from './types';

function compareFields(
  field: string,
  indexed: unknown,
  onchain: unknown,
  mismatches: string[],
): void {
  if (indexed !== onchain) {
    mismatches.push(`${field}: indexed=${JSON.stringify(indexed)} onchain=${JSON.stringify(onchain)}`);
  }
}

/**
 * A single per-user wrap divergence found by the sampling reconciler.
 *
 * Global counters can hide per-user drift (an indexer miss on one wrap is
 * invisible whenever the totals happen to match), and users experience their
 * own history — so each mismatch names the user, the period and what kind of
 * divergence it is, with enough record detail to debug.
 */
export interface PerUserMismatch {
  user: string;
  period: number;
  /**
   * 'missing': the wrap exists on-chain but not in the index (the indexer
   * missed it). 'extra': the wrap exists only in the index (stale row).
   * 'field_diff': both exist but disagree.
   */
  kind: 'missing' | 'extra' | 'field_diff';
  /** Populated for `field_diff`: the names of the disagreeing fields. */
  fields?: string[];
  /** The indexed wrap record, when one exists. */
  indexed?: WrapRecord | null;
  /** The on-chain wrap record, when one exists. */
  onchain?: WrapRecord | null;
}

/** Outcome of the per-user sampling comparison inside one reconciliation run. */
export interface PerUserComparison {
  /** Users that were compared this run. */
  sampled_users: string[];
  mismatches: PerUserMismatch[];
  /** Rows fixed by `--repair` during this run. */
  repaired: number;
}

export interface ReconcileOptions {
  /**
   * Maximum users to sample per run (a mix of recently-active and random
   * users, and users only visible on-chain). When omitted, every user is
   * compared. 0 disables the per-user comparison entirely.
   */
  sampleUsers?: number;
  /** Re-fetch mismatched wraps on-chain and fix the index in place. */
  repair?: boolean;
  /** Random source, injectable for deterministic tests. */
  random?: () => number;
}

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
    latest_ledger: number;
  };
  mismatches: string[];
  per_user: PerUserComparison;
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
 * Fisher-Yates shuffle driven by an injectable random source so sampling is
 * deterministic under test.
 */
function shuffle<T>(items: readonly T[], random: () => number): T[] {
  const arr = [...items];
  for (let i = arr.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [arr[i], arr[j]] = [arr[j], arr[i]];
  }
  return arr;
}

/**
 * Pick the users to compare this run: half the sample comes from recently
 * active users (the accounts an operator or user is most likely looking at),
 * the rest is random — drawing from indexed users and on-chain-only users
 * alike, so users the indexer missed entirely can still be detected.
 */
export function pickSampleUsers(
  db: IndexerDB,
  contractId: string,
  onChainUsers: readonly string[],
  sampleUsers: number,
  random: () => number = Math.random,
): string[] {
  if (sampleUsers <= 0) return [];

  const indexed = db.getUsers(contractId);
  const onChainOnly = onChainUsers.filter((u) => !indexed.includes(u));

  const sample: string[] = [];
  const seen = new Set<string>();
  const push = (user: string | undefined): void => {
    if (!user || seen.has(user) || sample.length >= sampleUsers) return;
    seen.add(user);
    sample.push(user);
  };

  const recentBudget = Math.ceil(sampleUsers / 2);
  for (const user of db.getRecentlyActiveUsers(contractId, recentBudget)) {
    push(user);
  }

  const restIndexed = shuffle(indexed.filter((u) => !seen.has(u)), random);
  const chainOnly = shuffle(onChainOnly, random);
  const rounds = Math.max(restIndexed.length, chainOnly.length);
  for (let i = 0; i < rounds && sample.length < sampleUsers; i++) {
    push(restIndexed[i]);
    if (sample.length < sampleUsers) push(chainOnly[i]);
  }

  return sample;
}

function rowToWrapRecord(row: WrapRow): WrapRecord {
  return {
    created_at: row.timestamp,
    data_hash: row.data_hash,
    archetype: row.archetype,
    period: row.period,
    lifecycle: {
      state: row.fsm_state as WrapState,
      updated_at: row.fsm_updated_at,
    },
  };
}

/** Names of the wrap fields where an indexed row disagrees with on-chain state. */
function wrapFieldDiffs(indexed: WrapRow, onchain: WrapRecord): string[] {
  const fields: string[] = [];
  if (indexed.timestamp !== onchain.created_at) fields.push('created_at');
  if (indexed.data_hash !== onchain.data_hash) fields.push('data_hash');
  if (indexed.archetype !== onchain.archetype) fields.push('archetype');
  if (indexed.fsm_state !== onchain.lifecycle.state) fields.push('lifecycle.state');
  if (indexed.fsm_updated_at !== onchain.lifecycle.updated_at) fields.push('lifecycle.updated_at');
  return fields;
}

/**
 * Compare a wrap row against the index and log the fix, keeping the report
 * small enough to read on failure.
 */
function upsertRepair(db: IndexerDB, contractId: string, mismatch: PerUserMismatch, record: WrapRecord, ledgerSeq: number): void {
  console.log(
    `repair: user=${mismatch.user} period=${mismatch.period} kind=${mismatch.kind}` +
      (mismatch.fields && mismatch.fields.length > 0 ? ` fields=${mismatch.fields.join(',')}` : '') +
      ` -> writing on-chain record`,
  );
  db.upsertWrap({
    contract_id: contractId,
    user: mismatch.user,
    period: mismatch.period,
    timestamp: record.created_at,
    data_hash: record.data_hash,
    archetype: record.archetype,
    fsm_state: record.lifecycle.state,
    fsm_updated_at: record.lifecycle.updated_at,
    ledger_seq: ledgerSeq,
    // Ledger entries carry no tx hash; the reconciler's job is to correct the
    // wrap content, not re-attribute its origin transaction.
    tx_hash: '',
  });
}

/**
 * Re-fetch the wrappable keys for every mismatch from the chain and write the
 * confirmed state back into the index. A freshly fetched record wins over the
 * cached snapshot; keys that could not be built fall back to the snapshot
 * fetched at the start of the run. Returns how many fixes were written.
 */
async function repairPerUserMismatches(
  db: IndexerDB,
  fetcher: SorobanFetcher,
  contractId: string,
  mismatches: PerUserMismatch[],
  onChainState: DerivedState,
): Promise<number> {
  const buildKey = (fetcher as unknown as {
    buildWrapKey?: (user: string, period: number) => xdr.LedgerKey;
  }).buildWrapKey;
  const fetchByKeys = (fetcher as unknown as {
    fetchStorageByKeys?: (keys: xdr.LedgerKey[]) => Promise<StorageEntry[]>;
  }).fetchStorageByKeys;

  // Map of `${user}:${period}` -> freshly fetched record, or null when the
  // re-fetch confirmed the key is absent on-chain. Absent means "we fetched
  // this key and it is not there", never "we could not look it up".
  const refetched = new Map<string, WrapRecord | null>();
  if (buildKey && fetchByKeys) {
    const keyed = new Map<string, { user: string; period: number }>();
    const ledgerKeys: xdr.LedgerKey[] = [];
    for (const mismatch of mismatches) {
      const id = `${mismatch.user}:${mismatch.period}`;
      try {
        ledgerKeys.push(buildKey.call(fetcher, mismatch.user, mismatch.period));
        keyed.set(id, { user: mismatch.user, period: mismatch.period });
      } catch (err) {
        console.error(
          `repair: could not build wrap key for user=${mismatch.user} period=${mismatch.period}; ` +
            `falling back to the cached on-chain snapshot:`,
          err instanceof Error ? err.message : err,
        );
      }
    }
    if (ledgerKeys.length > 0) {
      const entries = await fetchByKeys.call(fetcher, ledgerKeys);
      for (const entry of entries) {
        if (
          entry.key.variant === DataKeyVariant.Wrap &&
          entry.value.type === 'wrap_record'
        ) {
          const id = `${entry.key.user}:${entry.key.period}`;
          if (keyed.has(id)) refetched.set(id, entry.value.value);
        }
      }
    }
    for (const id of keyed.keys()) {
      if (!refetched.has(id)) refetched.set(id, null);
    }
  }

  let repaired = 0;
  const remaining: PerUserMismatch[] = [];
  for (const mismatch of mismatches) {
    const id = `${mismatch.user}:${mismatch.period}`;
    const snapshotRecord = onChainState.wraps.get(mismatch.user)?.get(mismatch.period) ?? null;
    // A completed re-fetch beats the cached snapshot; otherwise use it.
    const confirmed = refetched.has(id)
      ? refetched.get(id) ?? null
      : snapshotRecord;


    if (confirmed) {
      upsertRepair(db, contractId, mismatch, confirmed, onChainState.ledger_seq);
      repaired++;
      continue;
    }

    db.removeWrap(contractId, mismatch.user, mismatch.period);
    if (mismatch.kind === 'extra' || mismatch.kind === 'field_diff') {
      // The indexed row was the stale side: removing it repaired the index.
      console.log(
        `repair: user=${mismatch.user} period=${mismatch.period} kind=${mismatch.kind} -> removed stale indexed row`,
      );
      repaired++;
      continue;
    }
    if (mismatch.kind === 'missing' && refetched.has(id) && refetched.get(id) === null) {
      // Re-fetch confirmed neither side has this wrap: index and chain agree,
      // so the snapshot-time mismatch was already resolved. Drop the report.
      continue;
    }
    remaining.push(mismatch);
  }

  mismatches.length = 0;
  mismatches.push(...remaining);
  return repaired;
}

/**
 * Compare per-user wrap state for a sample of users: the indexed wraps in the
 * database against the wraps observed on-chain. Reports each divergence by
 * user, period and kind. With `repair`, the confirmed on-chain state is
 * written back into the index.
 */
export async function comparePerUser(
  db: IndexerDB,
  fetcher: SorobanFetcher,
  contractId: string,
  onChainState: DerivedState,
  options: ReconcileOptions = {},
): Promise<PerUserComparison> {
  const sampleUsers = options.sampleUsers ?? Number.MAX_SAFE_INTEGER;
  const random = options.random ?? Math.random;
  const sampled = pickSampleUsers(
    db,
    contractId,
    [...onChainState.wraps.keys()],
    sampleUsers,
    random,
  );

  const mismatches: PerUserMismatch[] = [];
  for (const user of sampled) {
    const indexedRows = db.getWrapsForUser(contractId, user);
    const indexedByPeriod = new Map(indexedRows.map((row) => [row.period, row] as const));
    const onChainWraps = onChainState.wraps.get(user) ?? new Map<number, WrapRecord>();

    for (const [period, onchainRecord] of onChainWraps) {
      const indexed = indexedByPeriod.get(period);
      if (!indexed) {
        mismatches.push({ user, period, kind: 'missing', onchain: onchainRecord });
        continue;
      }
      const fields = wrapFieldDiffs(indexed, onchainRecord);
      if (fields.length > 0) {
        mismatches.push({
          user,
          period,
          kind: 'field_diff',
          fields,
          indexed: rowToWrapRecord(indexed),
          onchain: onchainRecord,
        });
      }
    }

    for (const row of indexedRows) {
      if (!onChainWraps.has(row.period)) {
        mismatches.push({ user, period: row.period, kind: 'extra', indexed: rowToWrapRecord(row) });
      }
    }
  }

  let repaired = 0;
  if (options.repair && mismatches.length > 0) {
    repaired = await repairPerUserMismatches(db, fetcher, contractId, mismatches, onChainState);
  }

  return {
    sampled_users: sampled,
    mismatches,
    repaired,
  };
}

/**
 * Reconcile indexed state against current on-chain storage.
 * Fetches all current storage entries, compares global counters, and (per
 * issue #1006) samples per-user state so a wrap the indexer missed is caught
 * even when the global totals happen to match.
 */
export async function reconcile(
  db: IndexerDB,
  fetcher: SorobanFetcher,
  contractId: string,
  options: ReconcileOptions = {},
): Promise<ReconciliationReport> {
  const mismatches: string[] = [];
  const onChainState = createEmptyState(contractId, 0);

  // Fetch all current storage entries
  const storageEntries = await fetcher.fetchStorageEntries();

  // Build on-chain state from entries
  for (const entry of storageEntries) {
    onChainState.ledger_seq = Math.max(onChainState.ledger_seq, entry.ledger);
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

  const perUser = await comparePerUser(db, fetcher, contractId, onChainState, options);
  const isConsistent = mismatches.length === 0 && perUser.mismatches.length === 0;

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
      latest_ledger: onChainState.ledger_seq,
    },
    mismatches,
    per_user: perUser,
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
  // Per-user drift is its own alertable counter so it can be routed and
  // silenced independently of the global ones.
  for (const mismatch of report.per_user?.mismatches ?? []) {
    counters.add(`per_user_${mismatch.kind}`);
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
