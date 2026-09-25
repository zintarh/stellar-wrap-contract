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
  userWraps.set(period, record);

  // Increment wrap count
  const currentCount = state.userCounts.get(user) ?? 0;
  state.userCounts.set(user, currentCount + 1);
  state.totalWrapCount += 1;

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
      state.wraps.delete(user);
    }
  }

  const currentCount = state.userCounts.get(user) ?? 0;
  if (currentCount > 0) {
    state.userCounts.set(user, currentCount - 1);
  }
  state.totalRevoked += 1;

  // Remove from periods list
  const periods = state.userPeriods.get(user) ?? [];
  const idx = periods.indexOf(period);
  if (idx !== -1) {
    periods.splice(idx, 1);
    state.userPeriods.set(user, periods);
  }
}

function applyTransitionEvent(state: DerivedState, event: TypedEvent): void {
  const user = String(event.parsed.user ?? '');
  const period = Number(event.parsed.period ?? 0);
  const nextState = Number(event.parsed.next_state ?? 0);

  const userWraps = state.wraps.get(user);
  if (userWraps) {
    const record = userWraps.get(period);
    if (record) {
      record.fsm.state = nextState as WrapRecord['fsm']['state'];
      record.fsm.updated_at = event.raw.ledger;
    }
  }
}

// ─── Apply storage entries to state ────────────────────────────────────

export function applyStorageEntryToState(state: DerivedState, entry: StorageEntry): void {
  switch (entry.key.variant) {
    case DataKeyVariant.Admin:
      state.admin = entry.value.type === 'address' ? entry.value.value : null;
      break;
    case DataKeyVariant.AdminPubKey:
      state.adminPubKey = entry.value.type === 'bytes32' ? entry.value.value : null;
      break;
    case DataKeyVariant.PendingAdmin:
      state.pendingAdmin = entry.value.type === 'address' ? entry.value.value : null;
      break;
    case DataKeyVariant.MigrationVersion:
      state.migrationVersion = entry.value.type === 'u32' ? entry.value.value : 0;
      break;
    case DataKeyVariant.Paused:
      state.paused = entry.value.type === 'bool' ? entry.value.value : false;
      break;
    case DataKeyVariant.Name:
      state.name = entry.value.type === 'string' ? entry.value.value : null;
      break;
    case DataKeyVariant.Symbol:
      state.symbol = entry.value.type === 'string' ? entry.value.value : null;
      break;
    case DataKeyVariant.StorageBytes:
      state.storageBytes = entry.value.type === 'u64' ? entry.value.value : 0;
      break;
    case DataKeyVariant.TotalWrapCount:
      state.totalWrapCount = entry.value.type === 'u32' ? entry.value.value : 0;
      break;
    case DataKeyVariant.TotalRevoked:
      state.totalRevoked = entry.value.type === 'u64' ? entry.value.value : 0;
      break;
    case DataKeyVariant.SlashThreshold:
      state.slashThreshold = entry.value.type === 'u32' ? entry.value.value : 3;
      break;
    case DataKeyVariant.Wrap: {
      if (entry.value.type === 'wrap_record') {
        const user = (entry.key as { user: string }).user;
        const period = (entry.key as { period: number }).period;
        let userWraps = state.wraps.get(user);
        if (!userWraps) {
          userWraps = new Map();
          state.wraps.set(user, userWraps);
        }
        userWraps.set(period, entry.value.value);
      }
      break;
    }
    case DataKeyVariant.WrapCount: {
      const user = (entry.key as { user: string }).user;
      state.userCounts.set(user, entry.value.type === 'u32' ? entry.value.value : 0);
      break;
    }
    case DataKeyVariant.LatestPeriod: {
      const user = (entry.key as { user: string }).user;
      state.userLatestPeriods.set(user, entry.value.type === 'u64' ? entry.value.value : 0);
      break;
    }
    case DataKeyVariant.UserPeriods: {
      const user = (entry.key as { user: string }).user;
      state.userPeriods.set(user, entry.value.type === 'u64_vec' ? entry.value.value : []);
      break;
    }
    case DataKeyVariant.AliasHash: {
      const user = (entry.key as { user: string }).user;
      state.userAliasHashes.set(user, entry.value.type === 'bytes32' ? entry.value.value : '');
      break;
    }
    case DataKeyVariant.SlashCount: {
      const user = (entry.key as { user: string }).user;
      state.userSlashCounts.set(user, entry.value.type === 'u32' ? entry.value.value : 0);
      break;
    }
    case DataKeyVariant.Slashed: {
      const user = (entry.key as { user: string }).user;
      state.userSlashed.set(user, entry.value.type === 'bool' ? entry.value.value : false);
      break;
    }
  }
}

// ─── Persist derived state to DB ───────────────────────────────────────

export function persistStateToDB(db: IndexerDB, state: DerivedState): void {
  // Persist contract state
  db.upsertContractState({
    contract_id: state.contract_id,
    admin: state.admin,
    admin_pubkey: state.adminPubKey,
    pending_admin: state.pendingAdmin,
    migration_version: state.migrationVersion,
    is_paused: state.paused,
    total_wrap_count: state.totalWrapCount,
    total_revoked: state.totalRevoked,
    storage_bytes: state.storageBytes,
    slash_threshold: state.slashThreshold,
    ledger_seq: state.ledger_seq,
  });

  // Persist wraps
  for (const [user, periodMap] of state.wraps.entries()) {
    for (const [period, record] of periodMap.entries()) {
      db.upsertWrap({
        contract_id: state.contract_id,
        user,
        period,
        timestamp: record.timestamp,
        data_hash: record.data_hash,
        archetype: record.archetype,
        fsm_state: record.fsm.state,
        fsm_updated_at: record.fsm.updated_at,
        ledger_seq: state.ledger_seq,
        tx_hash: '',
      });
    }
  }

  // Persist user state
  const allUsers = new Set([
    ...state.wraps.keys(),
    ...state.userCounts.keys(),
    ...state.userLatestPeriods.keys(),
    ...state.userPeriods.keys(),
    ...state.userAliasHashes.keys(),
    ...state.userSlashCounts.keys(),
    ...state.userSlashed.keys(),
  ]);

  for (const user of allUsers) {
    db.upsertUserState({
      contract_id: state.contract_id,
      user,
      wrap_count: state.userCounts.get(user) ?? 0,
      latest_period: state.userLatestPeriods.get(user) ?? null,
      alias_hash: state.userAliasHashes.get(user) ?? null,
      slash_count: state.userSlashCounts.get(user) ?? 0,
      is_slashed: state.userSlashed.get(user) ?? false,
      periods: state.userPeriods.get(user) ?? [],
      ledger_seq: state.ledger_seq,
    });
  }
}

// ─── Orchestration helper ──────────────────────────────────────────────

/**
 * Process a batch of raw contract events: classify, apply to state, and persist.
 */
export function processEventBatch(
  db: IndexerDB,
  state: DerivedState,
  events: ContractEvent[],
): number {
  let processed = 0;

  for (const event of events) {
    if (event.failed_call) continue;

    const typed = classifyEvent(event);
    applyEventToState(state, typed);

    // Persist the raw event
    db.insertEvent({
      id: event.id,
      contract_id: event.contract_id,
      event_type: typed.event_type,
      ledger_seq: event.ledger,
      tx_hash: event.tx_hash,
      topics_json: JSON.stringify(event.topics),
      data_json: JSON.stringify(event.data),
      failed_call: event.failed_call,
    });

    processed++;
  }

  // Persist derived state after batch
  state.ledger_seq = events.length > 0
    ? events[events.length - 1].ledger
    : state.ledger_seq;
  persistStateToDB(db, state);

  return processed;
}
