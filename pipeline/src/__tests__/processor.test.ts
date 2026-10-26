import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  classifyEvent,
  createEmptyState,
  applyEventToState,
  applyStorageEntryToState,
  persistStateToDB,
} from '../processor';
import { DataKeyVariant } from '../types';
import { IndexerDB } from '../db';
import type { ContractEvent, StorageEntry } from '../types';

function makeMockEvent(overrides: Partial<ContractEvent> = {}): ContractEvent {
  return {
    id: 'test-event-1',
    contract_id: 'CCXJKVHD5L3M5X7H3YZJ5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5Y5',
    ledger: 100,
    ledger_close_at: null,
    tx_hash: 'abc123',
    topics: [],
    data: { type: 'string', value: '' },
    failed_call: false,
    ...overrides,
  };
}

describe('classifyEvent', () => {
  it('classifies mint event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'mint' },
        { type: 'address', value: 'GUSER1' },
        { type: 'u64', value: 202501 },
      ],
      data: { type: 'string', value: 'arch' },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('mint');
    expect(typed.parsed.user).toBe('GUSER1');
    expect(typed.parsed.period).toBe(202501);
    expect(typed.parsed.archetype).toBe('arch');
  });

  it('classifies revoke event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'revoke' },
        { type: 'address', value: 'GUSER1' },
        { type: 'u64', value: 202501 },
      ],
      data: { type: 'bytes', value: 'ab'.repeat(32) },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('revoke');
    expect(typed.parsed.user).toBe('GUSER1');
    expect(typed.parsed.period).toBe(202501);
    expect(typed.parsed.reason_hash).toBe('ab'.repeat(32));
  });

  it('classifies transition event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'trans' },
        { type: 'address', value: 'GUSER1' },
        { type: 'u64', value: 202501 },
      ],
      data: { type: 'u64', value: 4 },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('transition');
    expect(typed.parsed.next_state).toBe(4);
  });

  it('classifies init event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'init' },
        { type: 'address', value: 'GADMIN' },
      ],
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('init');
    expect(typed.parsed.admin).toBe('GADMIN');
  });

  it('classifies pause event with paused direction and admin', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'pause' },
        { type: 'symbol', value: 'paused' },
      ],
      data: { type: 'address', value: 'GADMIN' },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('pause');
    expect(typed.parsed.paused).toBe(true);
    expect(typed.parsed.admin).toBe('GADMIN');
  });

  it('classifies unpause event with unpaused direction and admin', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'pause' },
        { type: 'symbol', value: 'unpaused' },
      ],
      data: { type: 'address', value: 'GADMIN' },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('pause');
    expect(typed.parsed.paused).toBe(false);
    expect(typed.parsed.admin).toBe('GADMIN');
  });

  it('classifies slash report event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'slash' },
        { type: 'symbol', value: 'report' },
      ],
      data: { type: 'string', value: '["GUSER1",3]' },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('slash_report');
  });

  it('classifies slash clear event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'slash' },
        { type: 'symbol', value: 'clear' },
      ],
      data: { type: 'address', value: 'GUSER1' },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('slash_clear');
    expect(typed.parsed.user).toBe('GUSER1');
  });

  it('classifies slash threshold event', () => {
    const event = makeMockEvent({
      topics: [
        { type: 'symbol', value: 'slash' },
        { type: 'symbol', value: 'thresh' },
      ],
      data: { type: 'u32', value: 5 },
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('slash_threshold');
    expect(typed.parsed.threshold).toBe(5);
  });

  it('returns unknown for unrecognized events', () => {
    const event = makeMockEvent({
      topics: [{ type: 'symbol', value: 'unknown_event' }],
    });
    const typed = classifyEvent(event);
    expect(typed.event_type).toBe('unknown');
  });
});

describe('createEmptyState', () => {
  it('creates an empty state with defaults', () => {
    const state = createEmptyState('CCONTRACT', 0);
    expect(state.contract_id).toBe('CCONTRACT');
    expect(state.totalWrapCount).toBe(0);
    expect(state.totalRevoked).toBe(0);
    expect(state.slashThreshold).toBe(3);
    expect(state.admin).toBeNull();
    expect(state.wraps.size).toBe(0);
    expect(state.userCounts.size).toBe(0);
  });
});

describe('applyEventToState', () => {
  it('applies mint event to state', () => {
    const state = createEmptyState('CCONTRACT', 100);
    const event = {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'mint' as const,
      parsed: { user: 'GUSER1', period: 202501, archetype: 'arch' },
    };

    applyEventToState(state, event);

    expect(state.totalWrapCount).toBe(1);
    expect(state.userCounts.get('GUSER1')).toBe(1);
    expect(state.userLatestPeriods.get('GUSER1')).toBe(202501);
    expect(state.wraps.has('GUSER1')).toBe(true);
    expect(state.wraps.get('GUSER1')?.has(202501)).toBe(true);
  });

  it('applies revoke event to state', () => {
    const state = createEmptyState('CCONTRACT', 100);

    // First mint, then revoke
    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'mint',
      parsed: { user: 'GUSER1', period: 202501, archetype: 'arch' },
    });

    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 101 }),
      event_type: 'revoke',
      parsed: { user: 'GUSER1', period: 202501, reason_hash: '' },
    });

    expect(state.totalWrapCount).toBe(1);
    expect(state.userCounts.get('GUSER1')).toBe(0);
    expect(state.totalRevoked).toBe(1);
    expect(state.wraps.has('GUSER1')).toBe(false);
  });

  it('applies pause event to state', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'pause',
      parsed: { paused: true },
    });
    expect(state.paused).toBe(true);
  });

  it('applies slash report and clear events', () => {
    const state = createEmptyState('CCONTRACT', 100);

    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'slash_report',
      parsed: { user: 'GUSER1', count: 3 },
    });

    expect(state.userSlashCounts.get('GUSER1')).toBe(3);

    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 101 }),
      event_type: 'slash_clear',
      parsed: { user: 'GUSER1' },
    });

    expect(state.userSlashCounts.has('GUSER1')).toBe(false);
    expect(state.userSlashed.has('GUSER1')).toBe(false);
  });

  it('applies slash threshold event', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'slash_threshold',
      parsed: { threshold: 10 },
    });
    expect(state.slashThreshold).toBe(10);
  });

  it('applies init event to state', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyEventToState(state, {
      raw: makeMockEvent({ ledger: 100 }),
      event_type: 'init',
      parsed: { admin: 'GADMIN' },
    });
    expect(state.admin).toBe('GADMIN');
  });
});

describe('applyStorageEntryToState', () => {
  it('applies Admin storage entry', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyStorageEntryToState(state, {
      key: { variant: DataKeyVariant.Admin },
      value: { type: 'address', value: 'GADMIN' },
      ledger: 100,
      durability: 'instance',
    });
    expect(state.admin).toBe('GADMIN');
  });

  it('applies Wrap storage entry', () => {
    const state = createEmptyState('CCONTRACT', 100);
    const wrapRecord = {
      created_at: 1000,
      data_hash: 'ab'.repeat(32),
      archetype: 'arch',
      period: 202501,
      lifecycle: { state: 3 as const, updated_at: 1000 },
    };

    applyStorageEntryToState(state, {
      key: { variant: DataKeyVariant.Wrap, user: 'GUSER1', period: 202501 },
      value: { type: 'wrap_record', value: wrapRecord },
      ledger: 100,
      durability: 'persistent',
    });

    expect(state.wraps.has('GUSER1')).toBe(true);
    expect(state.wraps.get('GUSER1')?.has(202501)).toBe(true);
  });

  it('applies SlashCount storage entry', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyStorageEntryToState(state, {
      key: { variant: DataKeyVariant.SlashCount, user: 'GUSER1' },
      value: { type: 'u32', value: 3 },
      ledger: 100,
      durability: 'persistent',
    });
    expect(state.userSlashCounts.get('GUSER1')).toBe(3);
  });

  it('applies Slashed storage entry', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyStorageEntryToState(state, {
      key: { variant: DataKeyVariant.Slashed, user: 'GUSER1' },
      value: { type: 'bool', value: true },
      ledger: 100,
      durability: 'persistent',
    });
    expect(state.userSlashed.get('GUSER1')).toBe(true);
  });

  it('applies SlashThreshold storage entry', () => {
    const state = createEmptyState('CCONTRACT', 100);
    applyStorageEntryToState(state, {
      key: { variant: DataKeyVariant.SlashThreshold },
      value: { type: 'u32', value: 10 },
      ledger: 100,
      durability: 'instance',
    });
    expect(state.slashThreshold).toBe(10);
  });
});

describe('persistStateToDB — wraps_by_period integration', () => {
  let db: IndexerDB;

  beforeEach(async () => {
    db = await IndexerDB.create();
  });

  afterEach(() => {
    db.close();
  });

  const contractId = 'CCONTRACT';

  it('populates wraps_by_period when persisting mint events', () => {
    const state = createEmptyState(contractId, 100);

    // Simulate a mint event for GUSER1 in period 202609 with archetype 'arch'
    applyEventToState(state, {
      raw: {
        id: 'evt-1',
        contract_id: contractId,
        ledger: 100,
        ledger_close_at: null,
        tx_hash: 'tx1',
        topics: [
          { type: 'symbol', value: 'mint' },
          { type: 'address', value: 'GUSER1' },
          { type: 'u64', value: 202609 },
        ],
        data: { type: 'string', value: 'arch' },
        failed_call: false,
      } as ContractEvent,
      event_type: 'mint',
      parsed: { user: 'GUSER1', period: 202609, archetype: 'arch' },
    });

    persistStateToDB(db, state, contractId, 100, []);

    const rows = db.getWrapsByPeriod(contractId, 202609);
    expect(rows).toHaveLength(1);
    expect(rows[0].user).toBe('GUSER1');
    expect(rows[0].archetype).toBe('arch');
    expect(rows[0].period).toBe(202609);
  });

  it('populates wraps_by_period with archetype distribution across periods', () => {
    const state = createEmptyState(contractId, 200);

    // Two users wrap in Sep, one in Oct
    applyEventToState(state, {
      raw: { id: 'evt-1', contract_id: contractId, ledger: 200, ledger_close_at: null, tx_hash: 'tx1', topics: [{ type: 'symbol', value: 'mint' }, { type: 'address', value: 'GUSER1' }, { type: 'u64', value: 202609 }], data: { type: 'string', value: 'arch' }, failed_call: false } as ContractEvent,
      event_type: 'mint',
      parsed: { user: 'GUSER1', period: 202609, archetype: 'arch' },
    });
    applyEventToState(state, {
      raw: { id: 'evt-2', contract_id: contractId, ledger: 200, ledger_close_at: null, tx_hash: 'tx2', topics: [{ type: 'symbol', value: 'mint' }, { type: 'address', value: 'GUSER2' }, { type: 'u64', value: 202609 }], data: { type: 'string', value: 'other' }, failed_call: false } as ContractEvent,
      event_type: 'mint',
      parsed: { user: 'GUSER2', period: 202609, archetype: 'other' },
    });
    applyEventToState(state, {
      raw: { id: 'evt-3', contract_id: contractId, ledger: 201, ledger_close_at: null, tx_hash: 'tx3', topics: [{ type: 'symbol', value: 'mint' }, { type: 'address', value: 'GUSER3' }, { type: 'u64', value: 202610 }], data: { type: 'string', value: 'arch' }, failed_call: false } as ContractEvent,
      event_type: 'mint',
      parsed: { user: 'GUSER3', period: 202610, archetype: 'arch' },
    });

    persistStateToDB(db, state, contractId, 201, []);

    const sepDist = db.getArchetypeDistributionByPeriod(contractId, 202609);
    expect(sepDist).toHaveLength(2);
    expect(sepDist[0]).toMatchObject({ archetype: 'arch', count: 1 });
    expect(sepDist[1]).toMatchObject({ archetype: 'other', count: 1 });

    const octDist = db.getArchetypeDistributionByPeriod(contractId, 202610);
    expect(octDist).toHaveLength(1);
    expect(octDist[0]).toMatchObject({ archetype: 'arch', count: 1 });
  });

  it('wraps_by_period is consistent with on-chain events after revoke', () => {
    const state = createEmptyState(contractId, 300);

    // Mint then revoke
    applyEventToState(state, {
      raw: { id: 'evt-1', contract_id: contractId, ledger: 300, ledger_close_at: null, tx_hash: 'tx1', topics: [{ type: 'symbol', value: 'mint' }, { type: 'address', value: 'GUSER1' }, { type: 'u64', value: 202609 }], data: { type: 'string', value: 'arch' }, failed_call: false } as ContractEvent,
      event_type: 'mint',
      parsed: { user: 'GUSER1', period: 202609, archetype: 'arch' },
    });
    applyEventToState(state, {
      raw: { id: 'evt-2', contract_id: contractId, ledger: 301, ledger_close_at: null, tx_hash: 'tx2', topics: [{ type: 'symbol', value: 'revoke' }, { type: 'address', value: 'GUSER1' }, { type: 'u64', value: 202609 }], data: { type: 'bytes', value: 'ab'.repeat(32) }, failed_call: false } as ContractEvent,
      event_type: 'revoke',
      parsed: { user: 'GUSER1', period: 202609, reason_hash: 'ab'.repeat(32) },
    });

    persistStateToDB(db, state, contractId, 301, []);

    // After revoke, the wrap should be removed from wraps_by_period
    const rows = db.getWrapsByPeriod(contractId, 202609);
    expect(rows).toHaveLength(0);
  });
});
