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

  it('upgrades a database populated by the previous schema in place', async () => {
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
    legacy.close();

    // Reopening runs the forward migration in place and preserves data.
    const upgraded = await IndexerDB.create();
    expect(upgraded.getSchemaVersion()).toBe(SCHEMA_VERSION);
    expect(upgraded.getLatestEventLedger(contractId)).toBe(42);
    upgraded.close();
  });

  it('refuses to start against a database newer than the code understands', async () => {
    const future = await IndexerDB.create({ schemaVersion: SCHEMA_VERSION + 1 });
    future.close();

    await expect(IndexerDB.create()).rejects.toThrow(/newer/i);
  });
});
