import * as dotenv from 'dotenv';
import * as path from 'path';
import { StrKey } from '@stellar/stellar-sdk';
import { z } from 'zod';
import type { IndexerConfig } from './types';

dotenv.config({ path: path.resolve(__dirname, '../.env') });

/**
 * Current schema version understood by this build of the indexer.
 *
 * Bump this whenever the derived-state schema changes in a way that requires
 * a migration. The version is persisted inside the database itself (see
 * pipeline/src/db.ts) so an existing database can be upgraded in place.
 *
 * Policy for breaking schema changes:
 *   - Additive / in-place changes MUST ship a forward migration so operators
 *     upgrade without re-indexing.
 *   - A change that cannot be migrated forward (e.g. a destructive reshape of
 *     derived state) MUST bump SCHEMA_VERSION and set
 *     SCHEMA_REQUIRES_REINDEX = true. The indexer will then refuse to start
 *     against an older database with a clear error instead of silently
 *     re-indexing, making the re-index an explicit operator decision.
 */
export const SCHEMA_VERSION = 1;

/**
 * When true, a database at an older schema version cannot be migrated forward
 * and the operator must re-index from scratch. This is surfaced as an explicit
 * startup refusal rather than an implicit wipe.
 */
export const SCHEMA_REQUIRES_REINDEX = false;

// ─── Configuration schema ───────────────────────────────────────────────

/**
 * A single rejected configuration value, tied to the environment variable that
 * produced it.
 */
export interface ConfigIssue {
  /** Environment variable name, e.g. `CONTRACT_ID`. */
  field: string;
  /** Human-readable reason the value was rejected. */
  message: string;
}

/**
 * Raised when the environment does not describe a runnable indexer.
 *
 * Every invalid field is collected before the error is thrown so an operator
 * sees the complete list of problems in a single run instead of discovering
 * them one restart at a time.
 */
export class ConfigValidationError extends Error {
  constructor(public readonly issues: ConfigIssue[]) {
    super(formatConfigIssues(issues));
    this.name = 'ConfigValidationError';
  }
}

/**
 * Render configuration problems as an operator-facing report.
 *
 * One line per invalid field, so the output doubles as a checklist of what to
 * fix.
 */
export function formatConfigIssues(issues: ConfigIssue[]): string {
  const header =
    issues.length === 1
      ? 'Invalid indexer configuration (1 problem):'
      : `Invalid indexer configuration (${issues.length} problems):`;

  return [
    header,
    '',
    ...issues.map((issue) => `  - ${issue.field}: ${issue.message}`),
    '',
    'Set the values above in pipeline/.env (see pipeline/.env.example) and start again.',
  ].join('\n');
}

/**
 * Variables whose value must never be echoed back in an error message:
 * a webhook URL can carry credentials in its query string and a network
 * passphrase is secret by association with the deployment it describes.
 */
const SECRET_FIELDS = new Set(['ALERT_WEBHOOK_URL', 'NETWORK_PASSPHRASE']);

/**
 * Quote the offending value so the operator can see what was actually read.
 * Long values are truncated and secret values are omitted entirely.
 */
function describeValue(field: string, value: string | undefined): string {
  if (!value || SECRET_FIELDS.has(field)) {
    return '';
  }
  const shown = value.length > 80 ? `${value.slice(0, 77)}…` : value;
  return ` (received ${JSON.stringify(shown)})`;
}

/**
 * True when the value is an absolute http(s) URL.
 */
function isHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

/**
 * True when the value is a Stellar contract id (StrKey `C…`).
 *
 * The SDK is used as the source of truth so the length and the checksum are
 * verified, not just the prefix.
 */
function isContractId(value: string): boolean {
  try {
    return StrKey.isValidContract(value);
  } catch {
    return false;
  }
}

/**
 * True when the value is any Stellar address: an account id (StrKey `G…`) or
 * a contract id (StrKey `C…`).
 */
function isStellarAddress(value: string): boolean {
  try {
    return StrKey.isValidEd25519PublicKey(value) || StrKey.isValidContract(value);
  } catch {
    return false;
  }
}

/**
 * A contract id to index. The only value without a default: the indexer has
 * nothing to do without it.
 */
const contractIdSchema = z
  .string()
  .trim()
  .min(1, 'is required (set it in pipeline/.env or export it)')
  .refine(
    (value) => value === '' || isContractId(value),
    'must be a valid Stellar contract id (StrKey "C…", 56 characters)',
  );

/**
 * An http(s) endpoint. An empty value means "not configured" and is accepted
 * for the fields that may be left blank.
 */
const httpUrlSchema = z
  .string()
  .trim()
  .refine(
    (value) => value === '' || isHttpUrl(value),
    'must be a valid http(s) URL (for example https://soroban-testnet.stellar.org)',
  );

/**
 * The network passphrase the contract is expected to live on. It carries no
 * format of its own, but a multi-line value always means a malformed `.env`.
 */
const networkPassphraseSchema = z
  .string()
  .trim()
  .refine((value) => value === '' || !/[\r\n]/.test(value), 'must be a single line');

/**
 * The database location. The indexer stores its state in a local SQLite file
 * (via sql.js), so this is a filesystem path rather than a connection URL. A
 * relative path is resolved against the current working directory.
 */
const dbPathSchema = z
  .string()
  .trim()
  .min(1, 'is required')
  .refine((value) => !value.includes('\0'), 'must not contain null bytes');

/**
 * How often the indexer polls the RPC endpoint. Zero would busy-loop the
 * endpoint, so it is rejected.
 */
const pollIntervalSchema = z
  .string()
  .trim()
  .regex(/^[1-9]\d*$/, 'must be a positive integer (1 or greater)')
  .refine(
    (value) => Number(value) <= 86_400_000,
    'must be at most 86400000 milliseconds (24 hours)',
  )
  .transform(Number);

/**
 * How many events are requested per RPC page.
 */
const eventPageSizeSchema = z
  .string()
  .trim()
  .regex(/^[1-9]\d*$/, 'must be a positive integer (1 or greater)')
  .refine((value) => Number(value) <= 10_000, 'must be at most 10000')
  .transform(Number);

/**
 * The first ledger to index on a fresh database. Zero is the genesis ledger and
 * is accepted so an operator can replay the chain from the start.
 */
const startLedgerSchema = z
  .string()
  .trim()
  .regex(/^(0|[1-9]\d*)$/, 'must be a non-negative integer (0 or greater)')
  .refine((value) => Number.isSafeInteger(Number(value)), 'must be a safe integer')
  .transform(Number);

/**
 * The lowest severity that is forwarded to the alert webhook.
 */
const alertSeveritySchema = z.enum(['info', 'warning', 'critical'], {
  message: 'must be one of "info", "warning" or "critical"',
});

/**
 * The admin address the contract is expected to have. Used to spot an
 * unexpected admin change, so it may be left blank when it is not relevant.
 */
const expectedAdminSchema = z
  .string()
  .trim()
  .refine(
    (value) => value === '' || isStellarAddress(value),
    'must be a valid Stellar address (StrKey "G…" or "C…")',
  );

/**
 * The full environment schema.
 *
 * Every field is required here: unset variables are replaced by their default
 * before validation, so the defaults are validated by the same rules as the
 * values an operator supplies.
 */
const envSchema = z.object({
  CONTRACT_ID: contractIdSchema,
  RPC_URL: httpUrlSchema,
  NETWORK_PASSPHRASE: networkPassphraseSchema,
  DB_PATH: dbPathSchema,
  POLL_INTERVAL_MS: pollIntervalSchema,
  EVENT_PAGE_SIZE: eventPageSizeSchema,
  START_LEDGER: startLedgerSchema,
  ALERT_WEBHOOK_URL: httpUrlSchema,
  ALERT_MIN_SEVERITY: alertSeveritySchema,
  EXPECTED_ADMIN: expectedAdminSchema,
});

/**
 * The value used when a variable is unset or set to an empty string.
 *
 * `CONTRACT_ID` has no default on purpose: it is the one value the indexer
 * cannot run without.
 */
const DEFAULTS: Readonly<Record<keyof z.input<typeof envSchema>, string>> = {
  CONTRACT_ID: '',
  RPC_URL: 'https://soroban-testnet.stellar.org',
  NETWORK_PASSPHRASE: '',
  DB_PATH: path.resolve(__dirname, '../indexer.db'),
  POLL_INTERVAL_MS: '5000',
  EVENT_PAGE_SIZE: '100',
  START_LEDGER: '1',
  ALERT_WEBHOOK_URL: '',
  ALERT_MIN_SEVERITY: 'warning',
  EXPECTED_ADMIN: '',
};

/** The environment the configuration is read from. */
export type EnvSource = Record<string, string | undefined>;

/**
 * Read the environment, trimming every value.
 *
 * Missing and blank values are both normalised to the empty string so a
 * variable that was explicitly left blank behaves exactly like one that was
 * never set.
 */
function readEnv(env: EnvSource): Record<string, string> {
  const raw: Record<string, string> = {};
  for (const [key, value] of Object.entries(env)) {
    raw[key] = value === undefined ? '' : value.trim();
  }
  return raw;
}

/**
 * Replace every unset or blank variable with its default.
 */
function applyDefaults(raw: Record<string, string>): Record<string, string> {
  const resolved: Record<string, string> = { ...raw };
  for (const [key, fallback] of Object.entries(DEFAULTS)) {
    if (!resolved[key]) {
      resolved[key] = fallback;
    }
  }
  return resolved;
}

/**
 * Group the schema issues by field so each invalid variable is reported
 * exactly once, with every reason it was rejected.
 */
function toIssues(
  issues: z.core.$ZodIssue[],
  raw: Record<string, string>,
): ConfigIssue[] {
  const byField = new Map<string, string[]>();

  for (const issue of issues) {
    const field = issue.path.length > 0 ? String(issue.path[0]) : 'configuration';
    const messages = byField.get(field) ?? [];
    messages.push(issue.message);
    byField.set(field, messages);
  }

  return [...byField.entries()].map(([field, messages]) => ({
    field,
    message: `${messages.join('; ')}${describeValue(field, raw[field])}`,
  }));
}

/**
 * Build the indexer configuration from an environment and an argument list.
 *
 * Throws {@link ConfigValidationError} listing every invalid field when the
 * configuration cannot be used.
 */
export function parseConfig(env: EnvSource, argv: string[] = []): IndexerConfig {
  const raw = applyDefaults(readEnv(env));
  const result = envSchema.safeParse(raw);

  if (!result.success) {
    throw new ConfigValidationError(toIssues(result.error.issues, raw));
  }

  const parsed = result.data;

  return {
    rpc_url: parsed.RPC_URL,
    network_passphrase: parsed.NETWORK_PASSPHRASE,
    contract_id: parsed.CONTRACT_ID,
    db_path: parsed.DB_PATH,
    poll_interval_ms: parsed.POLL_INTERVAL_MS,
    event_page_size: parsed.EVENT_PAGE_SIZE,
    start_ledger: parsed.START_LEDGER,
    backfill: argv.includes('--backfill'),
    reconcile_only: argv.includes('--reconcile-only'),
    alert_webhook_url: parsed.ALERT_WEBHOOK_URL,
    alert_min_severity: parsed.ALERT_MIN_SEVERITY,
    expected_admin: parsed.EXPECTED_ADMIN,
  };
}

/**
 * Build the indexer configuration from the current process environment.
 *
 * Values come from `pipeline/.env` (loaded above) and the process environment,
 * with the process environment taking precedence.
 */
export function loadConfig(): IndexerConfig {
  return parseConfig(process.env, process.argv);
}
