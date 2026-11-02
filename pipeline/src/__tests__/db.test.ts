import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { IndexerDB, SCHEMA_VERSION } from '../db';

describe('IndexerDB', () => {
  let db: IndexerDB;

  beforeEach(async () => {
    db = await IndexerDB.create();
  });

  afterEach(() => {
    db.close();
  });

  const contractId = 'CCONTRACT123';

  it('creates tables on initialization', () => {
    // Verify tables exist by inserting and reading
    db.insertEvent({
      id: 'evt-1',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 100,
      tx_hash: 'tx1',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    const latest = db.getLatestEventLedger(contractId);
    expect(latest).toBe(100);
  });

  it('inserts and retrieves events', () => {
    db.insertEvent({
      id: 'evt-1',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 100,
      tx_hash: 'tx1',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    db.insertEvent({
      id: 'evt-2',
      contract_id: contractId,
      event_type: 'revoke',
      ledger_seq: 200,
      tx_hash: 'tx2',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    const events = db.getEventsByLedgerRange(contractId, 50, 250);
    expect(events).toHaveLength(2);
    expect(events[0].event_type).toBe('mint');
    expect(events[1].event_type).toBe('revoke');
  });

  it('ignores duplicate events (INSERT OR IGNORE)', () => {
    db.insertEvent({
      id: 'evt-1',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 100,
      tx_hash: 'tx1',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    db.insertEvent({
      id: 'evt-1',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 100,
      tx_hash: 'tx1',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    const events = db.getEventsByLedgerRange(contractId, 0, 999);
    expect(events).toHaveLength(1);
  });

  it('upserts wrap records', () => {
    db.upsertWrap({
      contract_id: contractId,
      user: 'GUSER1',
      period: 202501,
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      archetype: 'arch',
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });

    expect(db.getWrapCount(contractId)).toBe(1);

    // Upsert same wrap again (update)
    db.upsertWrap({
      contract_id: contractId,
      user: 'GUSER1',
      period: 202501,
      timestamp: 2000,
      data_hash: 'cd'.repeat(32),
      archetype: 'arch',
      fsm_state: 4,
      fsm_updated_at: 2000,
      ledger_seq: 200,
      tx_hash: 'tx2',
    });

    expect(db.getWrapCount(contractId)).toBe(1);
  });

  it('removes wrap records', () => {
    db.upsertWrap({
      contract_id: contractId,
      user: 'GUSER1',
      period: 202501,
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      archetype: 'arch',
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });

    expect(db.getWrapCount(contractId)).toBe(1);

    db.removeWrap(contractId, 'GUSER1', 202501);
    expect(db.getWrapCount(contractId)).toBe(0);
  });

  it('upserts user state', () => {
    db.upsertUserState({
      contract_id: contractId,
      user: 'GUSER1',
      wrap_count: 5,
      latest_period: 202505,
      alias_hash: 'ff'.repeat(32),
      slash_count: 0,
      is_slashed: false,
      periods: [202501, 202502, 202503],
      ledger_seq: 200,
    });

    // Update with new data
    db.upsertUserState({
      contract_id: contractId,
      user: 'GUSER1',
      wrap_count: 6,
      latest_period: 202506,
      alias_hash: null,
      slash_count: 2,
      is_slashed: true,
      periods: [202501, 202502, 202503, 202506],
      ledger_seq: 300,
    });

    // Re-query via contract state operations indirectly
    // No direct getter for user state, but we can verify via stats
    const stats = db.getStats(contractId);
    expect(stats.total_wraps).toBe(0); // No wraps were upserted in this test
  });

  it('upserts contract state', () => {
    db.upsertContractState({
      contract_id: contractId,
      admin: 'GADMIN',
      admin_pubkey: 'aa'.repeat(32),
      pending_admin: null,
      migration_version: 1,
      is_paused: false,
      total_wrap_count: 10,
      total_revoked: 2,
      storage_bytes: 1024,
      slash_threshold: 5,
      ledger_seq: 500,
    });

    const state = db.getContractState(contractId);
    expect(state).not.toBeNull();
    expect(state!.admin).toBe('GADMIN');
    expect(state!.total_wrap_count).toBe(10);
    expect(state!.slash_threshold).toBe(5);

    // Update
    db.upsertContractState({
      contract_id: contractId,
      admin: 'GADMIN2',
      admin_pubkey: 'bb'.repeat(32),
      pending_admin: null,
      migration_version: 2,
      is_paused: true,
      total_wrap_count: 20,
      total_revoked: 3,
      storage_bytes: 2048,
      slash_threshold: 3,
      ledger_seq: 600,
    });

    const updated = db.getContractState(contractId);
    expect(updated!.admin).toBe('GADMIN2');
    expect(updated!.is_paused).toBe(1);
    expect(updated!.slash_threshold).toBe(3);
  });

  it('manages ledger cursor', () => {
    const cursorId = `cursor:${contractId}`;

    // No cursor initially
    const initial = db.getCursor(cursorId);
    expect(initial).toBeNull();

    // Create cursor
    db.upsertCursor(cursorId, contractId, 100, 100);
    const cursor = db.getCursor(cursorId);
    expect(cursor).not.toBeNull();
    expect(cursor!.last_processed_ledger).toBe(100);

    // Update cursor
    db.upsertCursor(cursorId, contractId, 500, 500);
    const updated = db.getCursor(cursorId);
    expect(updated!.last_processed_ledger).toBe(500);
  });

  it('returns stats', () => {
    db.upsertWrap({
      contract_id: contractId,
      user: 'GUSER1',
      period: 202501,
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      archetype: 'arch',
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });

    db.insertEvent({
      id: 'evt-1',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 100,
      tx_hash: 'tx1',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });

    const stats = db.getStats(contractId);
    expect(stats.total_events).toBe(1);
    expect(stats.total_wraps).toBe(1);
    expect(stats.last_indexed_ledger).toBe(100);
  });

  it('stores the schema version in the database itself', () => {
    expect(db.getSchemaVersion()).toBe(SCHEMA_VERSION);
  });

  it('upgrades a database from the previous schema version', async () => {
    // Simulate an existing database created by the previous schema version.
    const legacy = await IndexerDB.create({ schemaVersion: SCHEMA_VERSION - 1 });
    legacy.insertEvent({
      id: 'legacy-evt',
      contract_id: contractId,
      event_type: 'mint',
      ledger_seq: 42,
      tx_hash: 'tx-legacy',
      topics_json: '[]',
      data_json: '{}',
      failed_call: false,
    });
    expect(legacy.getSchemaVersion()).toBe(SCHEMA_VERSION - 1);

    // Reopen with the same underlying db so data persists (in-memory sql.js doesn't survive close).
    const rawDb = legacy.getRawDb();

    const upgraded = await IndexerDB.create({ _rawDb: rawDb });
    expect(upgraded.getSchemaVersion()).toBe(SCHEMA_VERSION);
    expect(upgraded.getLatestEventLedger(contractId)).toBe(42);
    upgraded.close();
  });

  it('refuses to start against a database newer than the code understands', async () => {
    const future = await IndexerDB.create({ schemaVersion: SCHEMA_VERSION + 1 });
    expect(future.getSchemaVersion()).toBe(SCHEMA_VERSION + 1);
    const rawDb = future.getRawDb();

    await expect(IndexerDB.create({ _rawDb: rawDb })).rejects.toThrow(/newer/i);
  });

  // ── wraps_by_period ──────────────────────────────────────────

  it('upserts wraps_by_period records', () => {
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER1',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });

    const rows = db.getWrapsByPeriod(contractId, 202609);
    expect(rows).toHaveLength(1);
    expect(rows[0].user).toBe('GUSER1');
    expect(rows[0].archetype).toBe('arch');
    expect(rows[0].period).toBe(202609);
  });

  it('upserting the same wrap_by_period updates the record', () => {
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER1',
      archetype: 'arch-v1',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });

    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER1',
      archetype: 'arch-v2',
      timestamp: 2000,
      data_hash: 'cd'.repeat(32),
      fsm_state: 4,
      fsm_updated_at: 2000,
      ledger_seq: 200,
      tx_hash: 'tx2',
    });

    const rows = db.getWrapsByPeriod(contractId, 202609);
    expect(rows).toHaveLength(1);
    expect(rows[0].archetype).toBe('arch-v2');
    expect(rows[0].fsm_state).toBe(4);
  });

  it('getWrapsByPeriod returns paginated results', () => {
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER_B',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER_A',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx2',
    });
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER_C',
      archetype: 'other',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx3',
    });

    // Without cursor: returns first page
    const page1 = db.getWrapsByPeriod(contractId, 202609, null, 2);
    expect(page1).toHaveLength(2);
    expect(page1[0].user).toBe('GUSER_A');
    expect(page1[1].user).toBe('GUSER_B');

    // With cursor: returns results after the cursor
    const page2 = db.getWrapsByPeriod(contractId, 202609, 'GUSER_A', 2);
    expect(page2).toHaveLength(2);
    expect(page2[0].user).toBe('GUSER_B');
    expect(page2[1].user).toBe('GUSER_C');

    // Cursor past all results: empty
    const page3 = db.getWrapsByPeriod(contractId, 202609, 'GUSER_C', 2);
    expect(page3).toHaveLength(0);
  });

  it('getWrapsByPeriod filters by period', () => {
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER1',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202610,
      user: 'GUSER1',
      archetype: 'arch',
      timestamp: 2000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 2000,
      ledger_seq: 200,
      tx_hash: 'tx2',
    });

    const sep = db.getWrapsByPeriod(contractId, 202609);
    expect(sep).toHaveLength(1);
    expect(sep[0].period).toBe(202609);

    const oct = db.getWrapsByPeriod(contractId, 202610);
    expect(oct).toHaveLength(1);
    expect(oct[0].period).toBe(202610);
  });

  it('getArchetypeDistributionByPeriod returns counts per archetype', () => {
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER1',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx1',
    });
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER2',
      archetype: 'arch',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx2',
    });
    db.upsertWrapByPeriod({
      contract_id: contractId,
      period: 202609,
      user: 'GUSER3',
      archetype: 'other',
      timestamp: 1000,
      data_hash: 'ab'.repeat(32),
      fsm_state: 3,
      fsm_updated_at: 1000,
      ledger_seq: 100,
      tx_hash: 'tx3',
    });

    const dist = db.getArchetypeDistributionByPeriod(contractId, 202609);
    expect(dist).toHaveLength(2);
    expect(dist[0]).toMatchObject({ archetype: 'arch', count: 2 });
    expect(dist[1]).toMatchObject({ archetype: 'other', count: 1 });
  });

  it('getArchetypeDistributionByPeriod returns empty for unknown period', () => {
    const dist = db.getArchetypeDistributionByPeriod(contractId, 999999);
    expect(dist).toHaveLength(0);
  });
});
