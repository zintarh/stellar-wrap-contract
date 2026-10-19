import Database from 'better-sqlite3';
import { Pool } from 'pg';

export interface IndexerState {
  key: string;
  value: string;
}

export interface IndexerStore {
  init(): Promise<void>;
  getState(key: string): Promise<string | null>;
  setState(key: string, value: string): Promise<void>;
  close(): Promise<void>;
}

const MIGRATIONS: string[] = [
  `CREATE TABLE IF NOT EXISTS indexer_state (
     key TEXT PRIMARY KEY,
     value TEXT NOT NULL
   );`,
];

async function runMigrations(
  hasMigrationTable: () => Promise<boolean>,
  createMigrationTable: () => Promise<void>,
  appliedMigrations: () => Promise<number[]>,
  applyMigration: (index: number, sql: string) => Promise<void>,
): Promise<void> {
  if (!(await hasMigrationTable())) {
    await createMigrationTable();
  }
  const applied = new Set(await appliedMigrations());
  for (let i = 0; i < MIGRATIONS.length; i++) {
    if (!applied.has(i)) {
      await applyMigration(i, MIGRATIONS[i]);
    }
  }
}

export class SqliteStore implements IndexerStore {
  private db: Database.Database | null = null;

  constructor(private readonly filePath: string) {}

  async init(): Promise<void> {
    this.db = new Database(this.filePath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = FULL');
    await runMigrations(
      async () => {
        const row = this.db!
          .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'")
          .get();
        return Boolean(row);
      },
      async () => {
        this.db!.exec(
          'CREATE TABLE IF NOT EXISTS schema_migrations (id INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);',
        );
      },
      async () => {
        const rows = this.db!.prepare('SELECT id FROM schema_migrations').all() as { id: number }[];
        return rows.map((r) => r.id);
      },
      async (index, sql) => {
        const tx = this.db!.transaction(() => {
          this.db!.exec(sql);
          this.db!
            .prepare('INSERT INTO schema_migrations (id, applied_at) VALUES (?, ?)')
            .run(index, new Date().toISOString());
        });
        tx();
      },
    );
  }

  async getState(key: string): Promise<string | null> {
    const row = this.db!.prepare('SELECT value FROM indexer_state WHERE key = ?').get(key) as
      | { value: string }
      | undefined;
    return row ? row.value : null;
  }

  async setState(key: string, value: string): Promise<void> {
    this.db!
      .prepare(
        'INSERT INTO indexer_state (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value',
      )
      .run(key, value);
  }

  async close(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.db = null;
    }
  }
}

export class PostgresStore implements IndexerStore {
  private pool: Pool | null = null;

  constructor(private readonly connectionString: string) {}

  async init(): Promise<void> {
    this.pool = new Pool({ connectionString: this.connectionString });
    await runMigrations(
      async () => {
        const res = await this.pool!.query(
          "SELECT to_regclass('public.schema_migrations') AS exists",
        );
        return res.rows[0].exists !== null;
      },
      async () => {
        await this.pool!.query(
          'CREATE TABLE IF NOT EXISTS schema_migrations (id INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);',
        );
      },
      async () => {
        const res = await this.pool!.query('SELECT id FROM schema_migrations');
        return res.rows.map((r: { id: number }) => r.id);
      },
      async (index, sql) => {
        const client = await this.pool!.connect();
        try {
          await client.query('BEGIN');
          await client.query(sql);
          await client.query('INSERT INTO schema_migrations (id, applied_at) VALUES ($1, $2)', [
            index,
            new Date().toISOString(),
          ]);
          await client.query('COMMIT');
        } catch (err) {
          await client.query('ROLLBACK');
          throw err;
        } finally {
          client.release();
        }
      },
    );
  }

  async getState(key: string): Promise<string | null> {
    const res = await this.pool!.query('SELECT value FROM indexer_state WHERE key = $1', [key]);
    return res.rows.length > 0 ? res.rows[0].value : null;
  }

  async setState(key: string, value: string): Promise<void> {
    await this.pool!.query(
      'INSERT INTO indexer_state (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value',
      [key, value],
    );
  }

  async close(): Promise<void> {
    if (this.pool) {
      await this.pool.end();
      this.pool = null;
    }
  }
}

export function createStoreFromEnv(env: NodeJS.ProcessEnv = process.env): IndexerStore {
  const adapter = env.INDEXER_STORE_ADAPTER ?? 'sqlite';
  if (adapter === 'postgres' || adapter === 'pg') {
    const connectionString = env.DATABASE_URL;
    if (!connectionString) {
      throw new Error('DATABASE_URL is required when INDEXER_STORE_ADAPTER=postgres');
    }
    return new PostgresStore(connectionString);
  }
  if (adapter === 'sqlite') {
    return new SqliteStore(env.INDEXER_SQLITE_PATH ?? 'indexer.sqlite');
  }
  throw new Error(`Unknown INDEXER_STORE_ADAPTER: ${adapter}`);
}
