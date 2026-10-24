import { describe, it, expect } from 'vitest';
import { createEmptyState, applyEventToState, persistStateToDB } from '../processor';
import { IndexerDB } from '../db';
import { periodArchetypes, archetypeRarity } from '../api';
import type { ContractEvent, TypedEvent, DerivedState } from '../types';

function raw(): ContractEvent {
  return {
    id: 'e1',
    contract_id: 'CCONTRACT',
    ledger: 100,
    ledger_close_at: null,
    tx_hash: 'tx',
    topics: [],
    data: { type: 'string', value: '' },
    failed_call: false,
  };
}

const mint = (user: string, period: number, archetype: string): TypedEvent => ({
  raw: raw(),
  event_type: 'mint',
  parsed: { user, period, archetype },
});

const revoke = (user: string, period: number): TypedEvent => ({
  raw: raw(),
  event_type: 'revoke',
  parsed: { user, period, reason_hash: '' },
});

// Recompute counts straight from the wraps map (the "on-chain sum").
function sumFromWraps(state: DerivedState) {
  const out = new Map<number, Map<string, number>>();
  for (const periodMap of state.wraps.values()) {
    for (const [period, rec] of periodMap) {
      const byArch = out.get(period) ?? new Map<string, number>();
      byArch.set(rec.archetype, (byArch.get(rec.archetype) ?? 0) + 1);
      out.set(period, byArch);
    }
  }
  return out;
}

describe('archetype counts', () => {
  it('increments on mint', () => {
    const s = createEmptyState('CCONTRACT');
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U2', 202501, 'diamond'));
    applyEventToState(s, mint('U3', 202501, 'gold'));
    expect(s.archetypeCounts.get(202501)?.get('diamond')).toBe(2);
    expect(s.archetypeCounts.get(202501)?.get('gold')).toBe(1);
  });

  it('decrements on revoke and drops empty periods', () => {
    const s = createEmptyState('CCONTRACT');
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, revoke('U1', 202501));
    expect(s.archetypeCounts.has(202501)).toBe(false);
  });

  it('moves the count when the same key is minted with a new archetype', () => {
    const s = createEmptyState('CCONTRACT');
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U1', 202501, 'gold'));
    expect(s.archetypeCounts.get(202501)?.get('diamond')).toBeUndefined();
    expect(s.archetypeCounts.get(202501)?.get('gold')).toBe(1);
  });

  it('ignores revoke of a wrap that does not exist', () => {
    const s = createEmptyState('CCONTRACT');
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, revoke('U2', 202501));
    expect(s.archetypeCounts.get(202501)?.get('diamond')).toBe(1);
  });

  it('matches the sum computed from wraps after mixed events', () => {
    const s = createEmptyState('CCONTRACT');
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U2', 202501, 'gold'));
    applyEventToState(s, mint('U1', 202502, 'gold'));
    applyEventToState(s, revoke('U2', 202501));
    applyEventToState(s, mint('U3', 202501, 'diamond'));
    expect(s.archetypeCounts).toEqual(sumFromWraps(s));
  });

  it('survives persist and reload', async () => {
    const db = await IndexerDB.create();
    const s = createEmptyState('CCONTRACT', 100);
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U2', 202501, 'gold'));
    persistStateToDB(db, s, 'CCONTRACT', 100);
    const loaded = db.getDerivedState('CCONTRACT');
    expect(loaded?.archetypeCounts).toEqual(s.archetypeCounts);
    db.close();
  });
});

describe('api helpers', () => {
  it('returns distribution and share for a period', async () => {
    const db = await IndexerDB.create();
    const s = createEmptyState('CCONTRACT', 100);
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U2', 202501, 'diamond'));
    applyEventToState(s, mint('U3', 202501, 'gold'));
    persistStateToDB(db, s, 'CCONTRACT', 100);

    const res = periodArchetypes(db, 'CCONTRACT', 202501);
    expect(res.total).toBe(3);
    expect(res.archetypes[0]).toMatchObject({ archetype: 'diamond', count: 2 });
    expect(res.archetypes[0].share).toBeCloseTo(2 / 3);
    db.close();
  });

  it('returns rarity across periods', async () => {
    const db = await IndexerDB.create();
    const s = createEmptyState('CCONTRACT', 100);
    applyEventToState(s, mint('U1', 202501, 'diamond'));
    applyEventToState(s, mint('U2', 202501, 'gold'));
    applyEventToState(s, mint('U1', 202502, 'diamond'));
    persistStateToDB(db, s, 'CCONTRACT', 100);

    const res = archetypeRarity(db, 'CCONTRACT', 'diamond');
    expect(res.periods).toEqual([
      { period: 202501, count: 1, total: 2, share: 0.5 },
      { period: 202502, count: 1, total: 1, share: 1 },
    ]);
    db.close();
  });
});
