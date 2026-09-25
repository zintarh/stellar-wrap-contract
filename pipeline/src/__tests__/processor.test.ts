import { describe, it, expect } from 'vitest';
import {
  classifyEvent,
  createEmptyState,
  applyEventToState,
  applyStorageEntryToState,
} from '../processor';
import { DataKeyVariant } from '../types';
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
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      archetype: 'arch',
      period: 202501,
      fsm: { state: 3 as const, updated_at: 1000 },
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
