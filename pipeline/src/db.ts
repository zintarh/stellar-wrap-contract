import initSqlJs, { SqlJsStatic, Database as SqlJsDatabase } from 'sql.js';
import type {
  EventRow,
  ContractStateRow,
  LedgerCursorRow,
  StorageEntry,
} from './types';

/**
 * Current schema version understood by this code. Bump this whenever a
 * forward migration is added below. The version is persisted inside the
 * database itself (in the `schema_meta` table) so it travels with the data.
 */
export const SCHEMA_VERSION = 2;

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

  static async create(dbPath?: string): Promise<IndexerDB> {
    const SQL: SqlJsStatic = await initSqlJs();
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
    return new IndexerDB(db);
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

    // v1 -> v2: baseline schema. `CREATE TABLE IF NOT EXISTS` makes this
    // idempotent for both fresh databases and existing v1 databases.
    if (stored < 2) {
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

  getLatestEventLedger(contractId: string

/* … truncated 1946 chars — edit only what you need near the top … */
