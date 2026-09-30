import initSqlJs, { SqlJsStatic, Database as SqlJsDatabase } from 'sql.js';
import type {
  EventRow,
  ContractStateRow,
  DerivedState,
  LedgerCursorRow,
  StorageEntry,
  WrapState,
} from './types';

/**
 * Current schema version understood by this code. Bump this whenever a
 * forward migration is added below. The version is persisted inside the
 * database itself (in the `schema_meta` table) so it travels with the data.
 */
export const SCHEMA_VERSION = 3;

/**
 * Thrown when the on-disk database was written by a newer version of the
 * indexer than this code understands. We refuse to start rather than risk
 * corrupt reads against an unknown schema.
 */
export class SchemaVersionError extends Error {
  constructor(public readonly found: number, public readonly supported: number) {
    super(
      `Database schema version ${found} is newer than the supported version ${supported}. ` +
        `Refusing to start to avoid corrupt reads. Upgrade the indexer or restore a compatible database.`,
    );
    this.name = 'SchemaVersionError';
  }
}

export class IndexerDB {
  private db: SqlJsDatabase;

  constructor(db: SqlJsDatabase) {
    this.db = db;
    this.db.run('PRAGMA foreign_keys = ON');
    this.migrate();
  }

  static async create(
    dbPathOrOptions?: string | { schemaVersion?: number },
  ): Promise<IndexerDB> {
    const SQL: SqlJsStatic = await initSqlJs();
    const dbPath = typeof dbPathOrOptions === 'string' ? dbPathOrOptions : undefined;
    let db: SqlJsDatabase;
    if (dbPath) {
      const fs = await import('fs');
      try {
        const buffer = fs.readFileSync(dbPath);
        db = new SQL.Database(buffer);
      } catch {
        db = new SQL.Database();
      }
    } else {
      db = new SQL.Database();
    }
    const indexedDb = new IndexerDB(db);
    if (typeof dbPathOrOptions === 'object' && dbPathOrOptions.schemaVersion !== undefined) {
      indexedDb.setStoredVersion(dbPathOrOptions.schemaVersion);
      return new IndexerDB(db);
    }
    return indexedDb;
  }

  /**
   * Reads the schema version stored in the database. A database that predates
   * versioning (no `schema_meta` table) is treated as version 1, which is the
   * implicit schema created by the original `migrate()` implementation.
   */
  private getStoredVersion(): number {
    const hasMeta = this.fetchOne(
      `SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'`,
    );
    if (!hasMeta) return 1;
    const row = this.fetchOne(`SELECT value FROM schema_meta WHERE key = 'schema_version'`);
    if (!row) return 1;
    const parsed = Number(row.value);
    return Number.isFinite(parsed) ? parsed : 1;
  }

  private setStoredVersion(version: number): void {
    this.db.run(`
      CREATE TABLE IF NOT EXISTS schema_meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      )
    `);
    this.db.run(
      `INSERT INTO schema_meta (key, value) VALUES ('schema_version', ?)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
      [String(version)],
    );
  }

  getSchemaVersion(): number {
    return this.getStoredVersion();
  }

  /**
   * Applies forward migrations in order until the database matches
   * SCHEMA_VERSION. Refuses to start against a database newer than this code.
   *
   * Breaking-change policy: migrations here are additive and upgrade in place.
   * If a future change cannot be expressed as a forward migration, it must
   * bump SCHEMA_VERSION and explicitly trigger a re-index from scratch (drop
   * derived tables and reset the ledger cursor) rather than silently reading
   * stale data. That decision is made here, not implicitly at read time.
   */
  private migrate(): void {
    const stored = this.getStoredVersion();
    if (stored > SCHEMA_VERSION) {
      throw new SchemaVersionError(stored, SCHEMA_VERSION);
    }

    // v1 -> v3: create the baseline tables and reconciliation history.
    // `CREATE TABLE IF NOT EXISTS` makes this safe for existing databases.
    if (stored < 3) {
      this.createBaselineSchema();
    }

    this.setStoredVersion(SCHEMA_VERSION);
  }

  private createBaselineSchema(): void {
    this.db.run(`
      CREATE TABLE IF NOT EXISTS contract_events (
        id TEXT PRIMARY KEY,
        contract_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        ledger_seq INTEGER NOT NULL,
        tx_hash TEXT NOT NULL,
        topics_json TEXT NOT NULL,
        data_json TEXT NOT NULL,
        failed_call INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_events_contract_ledger
      ON contract_events(contract_id, ledger_seq)`);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_events_type
      ON contract_events(event_type)`);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS wrap_records (
        contract_id TEXT NOT NULL,
        user TEXT NOT NULL,
        period INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        data_hash TEXT NOT NULL,
        archetype TEXT NOT NULL,
        fsm_state INTEGER NOT NULL,
        fsm_updated_at INTEGER NOT NULL,
        ledger_seq INTEGER NOT NULL,
        tx_hash TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (contract_id, user, period)
      )
    `);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_wraps_user
      ON wrap_records(contract_id, user)`);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_wraps_period
      ON wrap_records(contract_id, period)`);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS user_state (
        contract_id TEXT NOT NULL,
        user TEXT NOT NULL,
        wrap_count INTEGER NOT NULL DEFAULT 0,
        latest_period INTEGER,
        alias_hash TEXT,
        slash_count INTEGER NOT NULL DEFAULT 0,
        is_slashed INTEGER NOT NULL DEFAULT 0,
        periods_json TEXT NOT NULL DEFAULT '[]',
        ledger_seq INTEGER NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (contract_id, user)
      )
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS contract_state (
        contract_id TEXT PRIMARY KEY,
        admin TEXT,
        admin_pubkey TEXT,
        pending_admin TEXT,
        migration_version INTEGER NOT NULL DEFAULT 0,
        is_paused INTEGER NOT NULL DEFAULT 0,
        total_wrap_count INTEGER NOT NULL DEFAULT 0,
        total_revoked INTEGER NOT NULL DEFAULT 0,
        storage_bytes INTEGER NOT NULL DEFAULT 0,
        slash_threshold INTEGER NOT NULL DEFAULT 3,
        ledger_seq INTEGER NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS storage_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        contract_id TEXT NOT NULL,
        ledger_seq INTEGER NOT NULL,
        key_variant TEXT NOT NULL,
        key_json TEXT NOT NULL,
        value_type TEXT NOT NULL,
        value_json TEXT NOT NULL,
        durability TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_snapshots_ledger
      ON storage_snapshots(contract_id, ledger_seq)`);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS ledger_cursor (
        id TEXT PRIMARY KEY,
        contract_id TEXT NOT NULL,
        last_processed_ledger INTEGER NOT NULL DEFAULT 0,
        last_event_ledger INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);

    // Idempotency ledger: records which events have already been applied so
    // that replays, backfills, and retries cannot double-apply derived state.
    this.db.run(`
      CREATE TABLE IF NOT EXISTS applied_events (
        event_id TEXT PRIMARY KEY,
        contract_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        ledger_seq INTEGER NOT NULL,
        applied_at TEXT NOT NULL DEFAULT (datetime('now'))
      )
    `);
    this.db.run(`CREATE INDEX IF NOT EXISTS idx_applied_events_contract
      ON applied_events(contract_id, ledger_seq)`);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS reconciliation_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        contract_id TEXT NOT NULL,
        ran_at TEXT NOT NULL,
        ledger_seq INTEGER NOT NULL DEFAULT 0,
        indexed_ledger INTEGER NOT NULL DEFAULT 0,
        chain_head_ledger INTEGER NOT NULL DEFAULT 0,
        is_consistent INTEGER NOT NULL,
        has_divergence INTEGER NOT NULL,
        drifts TEXT NOT NULL DEFAULT '[]'
      )
    `);
  }

  private exec(sql: string, params: unknown[] = []): void {
    this.db.run(sql, params);
  }

  private fetchOne(sql: string, params: unknown[] = []): Record<string, unknown> | null {
    const stmt = this.db.prepare(sql);
    if (params.length > 0) stmt.bind(params);
    let row: Record<string, unknown> | null = null;
    if (stmt.step()) {
      row = stmt.getAsObject();
    }
    stmt.free();
    return row;
  }

  private fetchAll(sql: string, params: unknown[] = []): Record<string, unknown>[] {
    const stmt = this.db.prepare(sql);
    if (params.length > 0) stmt.bind(params);
    const rows: Record<string, unknown>[] = [];
    while (stmt.step()) {
      rows.push(stmt.getAsObject());
    }
    stmt.free();
    return rows;
  }

  // ─── Idempotency ────────────────────────────────────────────────────

  /**
   * Returns true if this event has not yet been applied and atomically
   * records it as applied. Callers must skip derived-state writes when this
   * returns false, which makes replay/backfill/retry safe at the key level.
   */
  markEventApplied(event: {
    id: string;
    contract_id: string;
    event_type: string;
    ledger_seq: number;
  }): boolean {
    const existing = this.fetchOne(
      `SELECT event_id FROM applied_events WHERE event_id = ?`,
      [event.id],
    );
    if (existing) return false;
    this.exec(
      `INSERT OR IGNORE INTO applied_events (event_id, contract_id, event_type, ledger_seq)
       VALUES (?, ?, ?, ?)`,
      [event.id, event.contract_id, event.event_type, event.ledger_seq],
    );
    return true;
  }

  isEventApplied(eventId: string): boolean {
    const row = this.fetchOne(
      `SELECT event_id FROM applied_events WHERE event_id = ?`,
      [eventId],
    );
    return row !== null;
  }

  getAppliedEventIds(contractId: string): string[] {
    const rows = this.fetchAll(
      `SELECT event_id FROM applied_events WHERE contract_id = ? ORDER BY ledger_seq ASC`,
      [contractId],
    ) as { event_id: string }[];
    return rows.map((r) => r.event_id);
  }

  // ─── Ledger cursor ──────────────────────────────────────────────────

  /**
   * Reads the durable cursor for a contract. Returns null when no cursor has
   * been persisted yet, which signals a first run (callers fall back to
   * START_LEDGER).
   */
  getLedgerCursor(contractId: string): LedgerCursorRow | null {
    const row = this.fetchOne(
      `SELECT * FROM ledger_cursor WHERE contract_id = ?`,
      [contractId],
    );
    return (row as unknown as LedgerCursorRow) ?? null;
  }

  /**
   * Persists the last fully-processed ledger together with the derived data
   * for that ledger in a single transaction, so the cursor and the data it
   * describes can never disagree across a restart.
   */
  commitLedger(
    contractId: string,
    lastProcessedLedger: number,
    lastEventLedger: number,
    applyDerived: () => void,
  ): void {
    this.db.run('BEGIN');
    try {
      applyDerived();
      this.exec(
        `INSERT INTO ledger_cursor (id, contract_id, last_processed_ledger, last_event_ledger, updated_at)
         VALUES (?, ?, ?, ?, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
           last_processed_ledger = excluded.last_processed_ledger,
           last_event_ledger = excluded.last_event_ledger,
           updated_at = datetime('now')`,
        [`cursor:${contractId}`, contractId, lastProcessedLedger, lastEventLedger],
      );
      this.db.run('COMMIT');
    } catch (err) {
      this.db.run('ROLLBACK');
      throw err;
    }
  }

  /**
   * Guards against a persisted cursor that is ahead of the chain's current
   * ledger (e.g. a reset/reorged network). Refuses to proceed instead of
   * spinning forever waiting for ledgers that will never arrive.
   */
  assertCursorNotAhead(contractId: string, chainLedger: number): void {
    const cursor = this.getLedgerCursor(contractId);
    if (cursor && cursor.last_processed_ledger > chainLedger) {
      throw new Error(
        `Persisted cursor for ${contractId} is at ledger ${cursor.last_processed_ledger}, ` +
          `ahead of the chain's current ledger ${chainLedger}. Refusing to proceed; ` +
          `the database may be from a different network or the chain was reset.`,
      );
    }
  }

  // ─── Events ─────────────────────────────────────────────────────────

  insertEvent(event: {
    id: string;
    contract_id: string;
    event_type: string;
    ledger_seq: number;
    tx_hash: string;
    topics_json: string;
    data_json: string;
    failed_call: boolean;
  }): void {
    this.exec(
      `INSERT OR IGNORE INTO contract_events
        (id, contract_id, event_type, ledger_seq, tx_hash, topics_json, data_json, failed_call)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
      [event.id, event.contract_id, event.event_type, event.ledger_seq, event.tx_hash, event.topics_json, event.data_json, event.failed_call ? 1 : 0],
    );
  }

  getEventsByLedgerRange(
    contractId: string,
    startLedger: number,
    endLedger: number,
    limit: number = 1000,
  ): EventRow[] {
    return this.fetchAll(
      `SELECT * FROM contract_events
       WHERE contract_id = ? AND ledger_seq >= ? AND ledger_seq <= ?
       ORDER BY ledger_seq ASC
       LIMIT ?`,
      [contractId, startLedger, endLedger, limit],
    ) as unknown as EventRow[];
  }

  getLatestEventLedger(contractId: string): number | null {
    const row = this.fetchOne(
      `SELECT MAX(ledger_seq) as max_ledger FROM contract_events WHERE contract_id = ?`,
      [contractId],
    ) as { max_ledger: number | null } | null;
    return row?.max_ledger ?? null;
  }

  upsertWrap(record: {
    contract_id: string;
    user: string;
    period: number;
    timestamp: number;
    data_hash: string;
    archetype: string;
    fsm_state: number;
    fsm_updated_at: number;
    ledger_seq: number;
    tx_hash: string;
  }): void {
    this.exec(
      `INSERT INTO wrap_records
        (contract_id, user, period, timestamp, data_hash, archetype, fsm_state, fsm_updated_at, ledger_seq, tx_hash)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(contract_id, user, period) DO UPDATE SET
        timestamp = excluded.timestamp,
        data_hash = excluded.data_hash,
        archetype = excluded.archetype,
        fsm_state = excluded.fsm_state,
        fsm_updated_at = excluded.fsm_updated_at,
        ledger_seq = excluded.ledger_seq,
        tx_hash = excluded.tx_hash,
        updated_at = datetime('now')`,
      [record.contract_id, record.user, record.period, record.timestamp, record.data_hash, record.archetype, record.fsm_state, record.fsm_updated_at, record.ledger_seq, record.tx_hash],
    );
  }

  removeWrap(contractId: string, user: string, period: number): void {
    this.exec(
      `DELETE FROM wrap_records WHERE contract_id = ? AND user = ? AND period = ?`,
      [contractId, user, period],
    );
  }

  getWrapCount(contractId: string): number {
    const row = this.fetchOne(
      `SELECT COUNT(*) as count FROM wrap_records WHERE contract_id = ?`,
      [contractId],
    ) as { count: number };
    return row.count;
  }

  upsertUserState(state: {
    contract_id: string;
    user: string;
    wrap_count: number;
    latest_period: number | null;
    alias_hash: string | null;
    slash_count: number;
    is_slashed: boolean;
    periods: number[];
    ledger_seq: number;
  }): void {
    this.exec(
      `INSERT INTO user_state
        (contract_id, user, wrap_count, latest_period, alias_hash, slash_count, is_slashed, periods_json, ledger_seq)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(contract_id, user) DO UPDATE SET
        wrap_count = excluded.wrap_count,
        latest_period = excluded.latest_period,
        alias_hash = excluded.alias_hash,
        slash_count = excluded.slash_count,
        is_slashed = excluded.is_slashed,
        periods_json = excluded.periods_json,
        ledger_seq = excluded.ledger_seq,
        updated_at = datetime('now')`,
      [state.contract_id, state.user, state.wrap_count, state.latest_period, state.alias_hash, state.slash_count, state.is_slashed ? 1 : 0, JSON.stringify(state.periods), state.ledger_seq],
    );
  }

  upsertContractState(state: {
    contract_id: string;
    admin: string | null;
    admin_pubkey: string | null;
    pending_admin: string | null;
    migration_version: number;
    is_paused: boolean;
    total_wrap_count: number;
    total_revoked: number;
    storage_bytes: number;
    slash_threshold: number;
    ledger_seq: number;
  }): void {
    this.exec(
      `INSERT INTO contract_state
        (contract_id, admin, admin_pubkey, pending_admin, migration_version, is_paused,
         total_wrap_count, total_revoked, storage_bytes, slash_threshold, ledger_seq)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(contract_id) DO UPDATE SET
        admin = excluded.admin,
        admin_pubkey = excluded.admin_pubkey,
        pending_admin = excluded.pending_admin,
        migration_version = excluded.migration_version,
        is_paused = excluded.is_paused,
        total_wrap_count = excluded.total_wrap_count,
        total_revoked = excluded.total_revoked,
        storage_bytes = excluded.storage_bytes,
        slash_threshold = excluded.slash_threshold,
        ledger_seq = excluded.ledger_seq,
        updated_at = datetime('now')`,
      [state.contract_id, state.admin, state.admin_pubkey, state.pending_admin, state.migration_version, state.is_paused ? 1 : 0, state.total_wrap_count, state.total_revoked, state.storage_bytes, state.slash_threshold, state.ledger_seq],
    );
  }

  getContractState(contractId: string): ContractStateRow | null {
    return this.fetchOne(
      `SELECT * FROM contract_state WHERE contract_id = ?`,
      [contractId],
    ) as unknown as ContractStateRow | null;
  }

  getDerivedState(contractId: string): DerivedState | null {
    const contract = this.getContractState(contractId);
    if (!contract) return null;

    const state: DerivedState = {
      contract_id: contractId,
      ledger_seq: contract.ledger_seq,
      wraps: new Map(),
      userCounts: new Map(),
      userLatestPeriods: new Map(),
      userPeriods: new Map(),
      userAliasHashes: new Map(),
      userSlashCounts: new Map(),
      userSlashed: new Map(),
      admin: contract.admin,
      adminPubKey: contract.admin_pubkey,
      pendingAdmin: contract.pending_admin,
      migrationVersion: contract.migration_version,
      paused: Boolean(contract.is_paused),
      totalWrapCount: contract.total_wrap_count,
      totalRevoked: contract.total_revoked,
      storageBytes: contract.storage_bytes,
      slashThreshold: contract.slash_threshold,
      name: null,
      symbol: null,
    };

    const wraps = this.fetchAll(
      `SELECT * FROM wrap_records WHERE contract_id = ?`,
      [contractId],
    );
    for (const row of wraps) {
      const user = String(row.user);
      const period = Number(row.period);
      let userWraps = state.wraps.get(user);
      if (!userWraps) {
        userWraps = new Map();
        state.wraps.set(user, userWraps);
      }
      userWraps.set(period, {
        created_at: Number(row.timestamp),
        data_hash: String(row.data_hash),
        archetype: String(row.archetype),
        period,
        lifecycle: {
          state: Number(row.fsm_state) as WrapState,
          updated_at: Number(row.fsm_updated_at),
        },
      });
    }

    const users = this.fetchAll(
      `SELECT * FROM user_state WHERE contract_id = ?`,
      [contractId],
    );
    for (const row of users) {
      const user = String(row.user);
      state.userCounts.set(user, Number(row.wrap_count));
      if (row.latest_period !== null) state.userLatestPeriods.set(user, Number(row.latest_period));
      if (row.alias_hash !== null) state.userAliasHashes.set(user, String(row.alias_hash));
      state.userSlashCounts.set(user, Number(row.slash_count));
      state.userSlashed.set(user, Boolean(row.is_slashed));
      state.userPeriods.set(user, JSON.parse(String(row.periods_json)) as number[]);
    }
    return state;
  }

  insertStorageSnapshot(entry: StorageEntry, contractId: string): void {
    this.exec(
      `INSERT INTO storage_snapshots
        (contract_id, ledger_seq, key_variant, key_json, value_type, value_json, durability)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
      [contractId, entry.ledger, String(entry.key.variant), JSON.stringify(entry.key), entry.value.type, JSON.stringify(entry.value), entry.durability],
    );
  }

  getCursor(id: string): LedgerCursorRow | null {
    return this.fetchOne(
      `SELECT * FROM ledger_cursor WHERE id = ?`,
      [id],
    ) as unknown as LedgerCursorRow | null;
  }

  upsertCursor(id: string, contractId: string, processedLedger: number, eventLedger: number): void {
    this.exec(
      `INSERT INTO ledger_cursor (id, contract_id, last_processed_ledger, last_event_ledger)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(id) DO UPDATE SET
        last_processed_ledger = excluded.last_processed_ledger,
        last_event_ledger = excluded.last_event_ledger,
        updated_at = datetime('now')`,
      [id, contractId, processedLedger, eventLedger],
    );
  }

  getStats(contractId: string): Record<string, unknown> {
    const eventCount = this.fetchOne(
      `SELECT COUNT(*) as count FROM contract_events WHERE contract_id = ?`,
      [contractId],
    ) as { count: number };
    const lastLedger = this.getLatestEventLedger(contractId);
    return {
      contract_id: contractId,
      total_events: eventCount.count,
      total_wraps: this.getWrapCount(contractId),
      total_users: Number((this.fetchOne(
        `SELECT COUNT(*) as count FROM user_state WHERE contract_id = ?`,
        [contractId],
      ) as { count: number }).count),
      last_indexed_ledger: lastLedger,
    };
  }

  recordReconciliation(run: {
    ran_at: string;
    contract_id: string;
    indexed_ledger: number;
    chain_head_ledger: number;
    is_consistent: boolean;
    has_divergence: boolean;
    drifts: string;
  }): void {
    this.exec(
      `INSERT INTO reconciliation_runs
        (contract_id, ran_at, indexed_ledger, chain_head_ledger, is_consistent, has_divergence, drifts)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
      [run.contract_id, run.ran_at, run.indexed_ledger, run.chain_head_ledger, run.is_consistent ? 1 : 0, run.has_divergence ? 1 : 0, run.drifts],
    );
  }

  recordReconciliationRun(run: {
    contract_id: string;
    ledger_seq: number;
    indexed_ledger_seq: number;
    chain_head_ledger: number;
    indexer_lagging: boolean;
    genuine_divergence: boolean;
    affected_counters: string[];
    is_consistent: boolean;
    timestamp: string;
  }): void {
    this.exec(
      `INSERT INTO reconciliation_runs
        (contract_id, ran_at, ledger_seq, indexed_ledger, chain_head_ledger, is_consistent, has_divergence, drifts)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
      [run.contract_id, run.timestamp, run.ledger_seq, run.indexed_ledger_seq, run.chain_head_ledger, run.is_consistent ? 1 : 0, run.genuine_divergence ? 1 : 0, JSON.stringify(run.affected_counters)],
    );
  }

  close(): void {
    this.db.close();
  }
}
