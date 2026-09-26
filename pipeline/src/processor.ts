import type {
  ContractEvent,
  EventTopic,
  TypedEvent,
  EventType,
  WrapRecord,
  StorageEntry,
  DerivedState,
} from './types';
import { DataKeyVariant } from './types';
import { SorobanFetcher } from './fetcher';
import { IndexerDB } from './db';
import { decodeDataKey, decodeStorageValue } from './decoder';

// ─── Event classification ──────────────────────────────────────────────

export function classifyEvent(event: ContractEvent): TypedEvent {
  if (event.topics.length < 1) {
    return { raw: event, event_type: 'unknown', parsed: {} };
  }

  const t0 = event.topics[0];
  if (t0.type !== 'symbol') {
    return { raw: event, event_type: 'unknown', parsed: {} };
  }

  switch (t0.value) {
    case 'mint':
      return classifyMintEvent(event);
    case 'revoke':
      return classifyRevokeEvent(event);
    case 'trans':
      return classifyTransitionEvent(event);
    case 'init':
      return {
        raw: event,
        event_type: 'init',
        parsed: { admin: event.topics[1]?.value },
      };
    case 'pause': {
      // Direction is encoded as a sub-topic: "paused" vs "unpaused". The
      // payload carries the acting admin, so pause signals no longer need to
      // be decoded from a boolean payload to know their direction.
      const direction = event.topics[1];
      return {
        raw: event,
        event_type: 'pause',
        parsed: {
          paused: direction.type === 'symbol' && direction.value === 'paused',
          admin: event.data.value,
        },
      };
    }
    case 'upgrade':
      return {
        raw: event,
        event_type: 'upgrade',
        parsed: { new_wasm_hash: event.data.value },
      };
    case 'admin': {
      const subType = event.topics[1];
      return {
        raw: event,
        event_type: 'admin_update',
        parsed: {
          action: subType.type === 'symbol' ? subType.value : 'updated',
          data: event.data.value,
        },
      };
    }
    case 'slash': {
      const subType = event.topics[1];
      if (subType.type === 'symbol') {
        switch (subType.value) {
          case 'report':
            return classifySlashReportEvent(event);
          case 'clear':
            return classifySlashClearEvent(event);
          case 'thresh':
            return classifySlashThresholdEvent(event);
        }
      }
      return { raw: event, event_type: 'unknown', parsed: {} };
    }
    default:
      return { raw: event, event_type: 'unknown', parsed: {} };
  }
}

function classifyMintEvent(event: ContractEvent): TypedEvent {
  const user = event.topics[1]?.value;
  const period = event.topics[2]?.value;
  const archetype = event.data.value;

  return {
    raw: event,
    event_type: 'mint',
    parsed: {
      user,
      period: typeof period === 'number' ? period : Number(period),
      archetype,
    },
  };
}

function classifyRevokeEvent(event: ContractEvent): TypedEvent {
  const user = event.topics[1]?.value;
  const period = event.topics[2]?.value;
  const reasonHash = event.data.value;

  return {
    raw: event,
    event_type: 'revoke',
    parsed: {
      user,
      period: typeof period === 'number' ? period : Number(period),
      reason_hash: reasonHash,
    },
  };
}

function classifyTransitionEvent(event: ContractEvent): TypedEvent {
  const user = event.topics[1]?.value;
  const period = event.topics[2]?.value;
  const nextState = event.data.value;

  return {
    raw: event,
    event_type: 'transition',
    parsed: {
      user,
      period: typeof period === 'number' ? period : Number(period),
      next_state: typeof nextState === 'number' ? nextState : Number(nextState),
    },
  };
}

function classifySlashReportEvent(event: ContractEvent): TypedEvent {
  const dataVal = event.data.value;
  let user = '';
  let count = 0;
  if (typeof dataVal === 'object' && dataVal !== null) {
    const arr = Array.isArray(dataVal) ? dataVal : Object.values(dataVal as any);
    if (arr.length >= 2) {
      user = String(arr[0]);
      count = Number(arr[1]);
    }
  }

  return {
    raw: event,
    event_type: 'slash_report',
    parsed: { user, count },
  };
}

function classifySlashClearEvent(event: ContractEvent): TypedEvent {
  return {
    raw: event,
    event_type: 'slash_clear',
    parsed: { user: event.data.value },
  };
}

function classifySlashThresholdEvent(event: ContractEvent): TypedEvent {
  return {
    raw: event,
    event_type: 'slash_threshold',
    parsed: { threshold: event.data.value },
  };
}

// ─── Event identity (idempotency key) ──────────────────────────────────

/**
 * Derive a stable identity for an event so that replaying the same event
 * (after a restart, a backfill, or a retry) can be detected and applied at
 * most once. The identity is built from the ledger/transaction coordinates
 * plus the event index within the transaction, which uniquely identifies an
 * event on-chain regardless of how many times it is fetched.
 */
export function eventId(event: ContractEvent): string {
  const ledger = (event as any).ledger ?? (event as any).ledger_seq ?? 0;
  const tx = (event as any).tx_hash ?? (event as any).transaction_hash ?? '';
  const idx = (event as any).event_index ?? (event as any).index ?? 0;
  return `${ledger}:${tx}:${idx}`;
}

// ─── State derivation from events ──────────────────────────────────────

export function createEmptyState(contractId: string, ledgerSeq: number): DerivedState {
  return {
    contract_id: contractId,
    ledger_seq: ledgerSeq,
    wraps: new Map(),
    userCounts: new Map(),
    userLatestPeriods: new Map(),
    userPeriods: new Map(),
    userAliasHashes: new Map(),
    userSlashCounts: new Map(),
    userSlashed: new Map(),
    admin: null,
    adminPubKey: null,
    pendingAdmin: null,
    migrationVersion: 0,
    paused: false,
    totalWrapCount: 0,
    totalRevoked: 0,
    storageBytes: 0,
    slashThreshold: 3,
    name: null,
    symbol: null,
  };
}

/**
 * Apply a stream of events to a fresh state, deduplicating by event identity.
 * Applying the same stream twice (or a stream with duplicates) yields the
 * same derived state as applying the deduplicated stream once.
 */
export function applyEventStream(
  contractId: string,
  ledgerSeq: number,
  events: TypedEvent[],
): DerivedState {
  const state = createEmptyState(contractId, ledgerSeq);
  const seen = new Set<string>();
  for (const event of events) {
    const id = eventId(event.raw);
    if (seen.has(id)) continue;
    seen.add(id);
    applyEventToState(state, event);
  }
  return state;
}

export function applyEventToState(state: DerivedState, event: TypedEvent): void {
  switch (event.event_type) {
    case 'mint':
      applyMintEvent(state, event);
      break;
    case 'revoke':
      applyRevokeEvent(state, event);
      break;
    case 'transition':
      applyTransitionEvent(state, event);
      break;
    case 'init':
      state.admin = String(event.parsed.admin ?? '');
      break;
    case 'pause':
      state.paused = Boolean(event.parsed.paused);
      break;
    case 'admin_update':
      if (event.parsed.action === 'updated' && Array.isArray(event.parsed.data)) {
        const data = event.parsed.data as [unknown, unknown];
        state.admin = String(data[1] ?? '');
      }
      break;
    case 'slash_report': {
      const user = String(event.parsed.user ?? '');
      const count = Number(event.parsed.count ?? 0);
      state.userSlashCounts.set(user, count);
      break;
    }
    case 'slash_clear': {
      const user = String(event.parsed.user ?? '');
      state.userSlashCounts.delete(user);
      state.userSlashed.delete(user);
      break;
    }
    case 'slash_threshold':
      state.slashThreshold = Number(event.parsed.threshold ?? 3);
      break;
    default:
      break;
  }
}

function applyMintEvent(state: DerivedState, event: TypedEvent): void {
  const user = String(event.parsed.user ?? '');
  const period = Number(event.parsed.period ?? 0);
  const archetype = String(event.parsed.archetype ?? '');

  // Create a placeholder wrap record from the mint event
  const record: WrapRecord = {
    timestamp: 0, // Not available from event; filled from storage
    data_hash: '', // Not available from event; filled from storage
    archetype,
    period,
    fsm: { state: 3, updated_at: 0 }, // Active by default
  };

  let userWraps = state.wraps.get(user);
  if (!userWraps) {
    userWraps = new Map();
    state.wraps.set(user, userWraps);
  }

  // Idempotent at the key level: only count a wrap the first time this
  // (user, period) key is observed. Re-applying the same mint must not
  // inflate the derived counts.
  const isNew = !userWraps.has(period);
  userWraps.set(period, record);

  if (isNew) {
    const currentCount = state.userCounts.get(user) ?? 0;
    state.userCounts.set(user, currentCount + 1);
    state.totalWrapCount += 1;
  }

  // Update latest period
  const currentLatest = state.userLatestPeriods.get(user) ?? 0;
  if (period > currentLatest) {
    state.userLatestPeriods.set(user, period);
  }

  // Add to periods list
  const periods = state.userPeriods.get(user) ?? [];
  if (!periods.includes(period)) {
    periods.push(period);
    periods.sort((a, b) => a - b);
    state.userPeriods.set(user, periods);
  }
}

function applyRevokeEvent(state: DerivedState, event: TypedEvent): void {
  const user = String(event.parsed.user ?? '');
  const period = Number(event.parsed.period ?? 0);

  const userWraps = state.wraps.get(user);
  if (userWraps) {
    userWraps.delete(period);
    if (userWraps.size === 0) {
      state.wraps

/* … truncated 7137 chars — edit only what you need near the top … */
