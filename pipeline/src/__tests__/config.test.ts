import { describe, it, expect, afterEach, vi } from 'vitest';
import {
  parseConfig,
  loadConfig,
  formatConfigIssues,
  ConfigValidationError,
  type ConfigIssue,
} from '../config';

const CONTRACT_ID = 'CC3GVOAVIKWIJPXZSAUZ545OCHUKGTJ7OKURU4PCUJWQM4MTZXU4VULZ';
const ADMIN_ID = 'GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVSGZ';

/**
 * A complete, valid environment. Tests override only the fields they care
 * about, so an unrelated variable in the ambient environment cannot influence
 * the result.
 */
function validEnv(): Record<string, string> {
  return {
    CONTRACT_ID,
    RPC_URL: 'https://rpc.example.com',
    NETWORK_PASSPHRASE: '',
    DB_PATH: './indexer.db',
    POLL_INTERVAL_MS: '5000',
    EVENT_PAGE_SIZE: '100',
    START_LEDGER: '1',
    ALERT_WEBHOOK_URL: '',
    ALERT_MIN_SEVERITY: 'warning',
    EXPECTED_ADMIN: '',
  };
}

/**
 * Parse an environment, expecting it to be rejected.
 */
function expectInvalid(env: Record<string, string | undefined>): ConfigIssue[] {
  try {
    parseConfig(env);
  } catch (err) {
    expect(err).toBeInstanceOf(ConfigValidationError);
    return (err as ConfigValidationError).issues;
  }
  throw new Error('expected the configuration to be rejected');
}

/**
 * The fields reported by a rejected environment, in the order they were
 * reported.
 */
function fieldsOf(issues: ConfigIssue[]): string[] {
  return issues.map((issue) => issue.field);
}

describe('parseConfig', () => {
  it('falls back to the documented defaults when only the contract id is set', () => {
    const config = parseConfig({ CONTRACT_ID });

    expect(config.contract_id).toBe(CONTRACT_ID);
    expect(config.rpc_url).toBe('https://soroban-testnet.stellar.org');
    expect(config.db_path).toMatch(/indexer\.db$/);
    expect(config.poll_interval_ms).toBe(5000);
    expect(config.event_page_size).toBe(100);
    expect(config.start_ledger).toBe(1);
    expect(config.alert_webhook_url).toBe('');
    expect(config.alert_min_severity).toBe('warning');
    expect(config.expected_admin).toBe('');
    expect(config.network_passphrase).toBe('');
    expect(config.backfill).toBe(false);
    expect(config.reconcile_only).toBe(false);
  });

  it('uses every configured value', () => {
    const config = parseConfig({
      ...validEnv(),
      NETWORK_PASSPHRASE: 'Test SDF Network ; September 2015',
      DB_PATH: '/var/lib/indexer/indexer.db',
      POLL_INTERVAL_MS: '2500',
      EVENT_PAGE_SIZE: '250',
      START_LEDGER: '42',
      ALERT_WEBHOOK_URL: 'https://hooks.example.com/stellar',
      ALERT_MIN_SEVERITY: 'critical',
      EXPECTED_ADMIN: ADMIN_ID,
    });

    expect(config).toMatchObject({
      rpc_url: 'https://rpc.example.com',
      network_passphrase: 'Test SDF Network ; September 2015',
      contract_id: CONTRACT_ID,
      db_path: '/var/lib/indexer/indexer.db',
      poll_interval_ms: 2500,
      event_page_size: 250,
      start_ledger: 42,
      alert_webhook_url: 'https://hooks.example.com/stellar',
      alert_min_severity: 'critical',
      expected_admin: ADMIN_ID,
    });
  });

  it('treats a blank value as unset and falls back to the default', () => {
    const config = parseConfig({ CONTRACT_ID, RPC_URL: '   ', POLL_INTERVAL_MS: '' });

    expect(config.rpc_url).toBe('https://soroban-testnet.stellar.org');
    expect(config.poll_interval_ms).toBe(5000);
  });

  it('reads the mode flags from the argument list', () => {
    expect(parseConfig({ CONTRACT_ID }, ['node', 'index.ts', '--backfill']).backfill).toBe(true);
    expect(
      parseConfig({ CONTRACT_ID }, ['node', 'index.ts', '--reconcile-only']).reconcile_only,
    ).toBe(true);
    expect(parseConfig({ CONTRACT_ID }, ['node', 'index.ts']).backfill).toBe(false);
  });

  it('rejects a missing contract id', () => {
    const issues = expectInvalid({});

    expect(fieldsOf(issues)).toEqual(['CONTRACT_ID']);
    expect(issues[0].message).toContain('is required');
  });

  it('rejects a contract id that is not a Stellar StrKey', () => {
    const issues = expectInvalid({ CONTRACT_ID: 'not-a-contract-id' });

    expect(fieldsOf(issues)).toEqual(['CONTRACT_ID']);
    expect(issues[0].message).toContain('must be a valid Stellar contract id');
    expect(issues[0].message).toContain('not-a-contract-id');
  });

  it('rejects a contract id in lower case', () => {
    const issues = expectInvalid({ CONTRACT_ID: CONTRACT_ID.toLowerCase() });

    expect(fieldsOf(issues)).toEqual(['CONTRACT_ID']);
  });

  it('rejects a malformed rpc url', () => {
    const issues = expectInvalid({ CONTRACT_ID, RPC_URL: 'not a url' });

    expect(fieldsOf(issues)).toEqual(['RPC_URL']);
    expect(issues[0].message).toContain('must be a valid http(s) URL');
  });

  it('rejects an rpc url that is not http(s)', () => {
    const issues = expectInvalid({ CONTRACT_ID, RPC_URL: 'ftp://rpc.example.com' });

    expect(fieldsOf(issues)).toEqual(['RPC_URL']);
  });

  it('rejects a poll interval that is not a positive integer', () => {
    for (const value of ['0', '-1', '1.5', 'abc']) {
      const issues = expectInvalid({ CONTRACT_ID, POLL_INTERVAL_MS: value });

      expect(fieldsOf(issues)).toEqual(['POLL_INTERVAL_MS']);
      expect(issues[0].message).toContain('must be a positive integer');
    }
  });

  it('rejects a poll interval beyond the supported maximum', () => {
    const issues = expectInvalid({ CONTRACT_ID, POLL_INTERVAL_MS: '86400001' });

    expect(fieldsOf(issues)).toEqual(['POLL_INTERVAL_MS']);
    expect(issues[0].message).toContain('at most');
  });

  it('rejects an event page size that is not a positive integer', () => {
    const issues = expectInvalid({ CONTRACT_ID, EVENT_PAGE_SIZE: '0' });

    expect(fieldsOf(issues)).toEqual(['EVENT_PAGE_SIZE']);
  });

  it('rejects a negative start ledger', () => {
    const issues = expectInvalid({ CONTRACT_ID, START_LEDGER: '-1' });

    expect(fieldsOf(issues)).toEqual(['START_LEDGER']);
    expect(issues[0].message).toContain('must be a non-negative integer');
  });

  it('rejects an unknown alert severity', () => {
    const issues = expectInvalid({ CONTRACT_ID, ALERT_MIN_SEVERITY: 'verbose' });

    expect(fieldsOf(issues)).toEqual(['ALERT_MIN_SEVERITY']);
    expect(issues[0].message).toContain('must be one of');
  });

  it('rejects a malformed alert webhook url', () => {
    const issues = expectInvalid({ CONTRACT_ID, ALERT_WEBHOOK_URL: 'hooks.example.com' });

    expect(fieldsOf(issues)).toEqual(['ALERT_WEBHOOK_URL']);
  });

  it('rejects an expected admin that is not a Stellar address', () => {
    const issues = expectInvalid({ CONTRACT_ID, EXPECTED_ADMIN: 'admin' });

    expect(fieldsOf(issues)).toEqual(['EXPECTED_ADMIN']);
    expect(issues[0].message).toContain('must be a valid Stellar address');
  });

  it('rejects a network passphrase that spans several lines', () => {
    const issues = expectInvalid({ CONTRACT_ID, NETWORK_PASSPHRASE: 'secret\npassphrase' });

    expect(fieldsOf(issues)).toEqual(['NETWORK_PASSPHRASE']);
    expect(issues[0].message).toContain('must be a single line');
  });

  it('reports every invalid field at once', () => {
    const issues = expectInvalid({
      CONTRACT_ID: 'nope',
      RPC_URL: 'not a url',
      POLL_INTERVAL_MS: '0',
      ALERT_MIN_SEVERITY: 'verbose',
      EXPECTED_ADMIN: 'admin',
    });

    expect(fieldsOf(issues).sort()).toEqual(
      ['ALERT_MIN_SEVERITY', 'CONTRACT_ID', 'EXPECTED_ADMIN', 'POLL_INTERVAL_MS', 'RPC_URL'].sort(),
    );
  });

  it('never echoes secret values back to the operator', () => {
    const issues = expectInvalid({
      CONTRACT_ID,
      ALERT_WEBHOOK_URL: 'hooks.example.com?token=super-secret',
      NETWORK_PASSPHRASE: 'super-secret passphrase\nsecond line',
    });

    const rendered = formatConfigIssues(issues);

    expect(rendered).toContain('ALERT_WEBHOOK_URL');
    expect(rendered).toContain('NETWORK_PASSPHRASE');
    expect(rendered).not.toContain('super-secret');
  });

  it('renders one line per invalid field', () => {
    const issues = expectInvalid({ CONTRACT_ID: 'nope', RPC_URL: 'not a url' });
    const rendered = formatConfigIssues(issues);

    expect(rendered).toContain('Invalid indexer configuration (2 problems):');
    expect(rendered).toContain('- CONTRACT_ID:');
    expect(rendered).toContain('- RPC_URL:');
    expect(rendered).toContain('pipeline/.env.example');
  });
});

describe('loadConfig', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it('reads the configuration from the process environment', () => {
    for (const [key, value] of Object.entries(validEnv())) {
      vi.stubEnv(key, value);
    }

    const config = loadConfig();

    expect(config.contract_id).toBe(CONTRACT_ID);
    expect(config.poll_interval_ms).toBe(5000);
  });

  it('fails with field-level messages when the environment is invalid', () => {
    for (const [key, value] of Object.entries({
      ...validEnv(),
      CONTRACT_ID: '',
      RPC_URL: 'not a url',
    })) {
      vi.stubEnv(key, value);
    }

    try {
      loadConfig();
      throw new Error('expected loadConfig to reject the environment');
    } catch (err) {
      expect(err).toBeInstanceOf(ConfigValidationError);
      const { issues, message } = err as ConfigValidationError;
      expect(fieldsOf(issues).sort()).toEqual(['CONTRACT_ID', 'RPC_URL']);
      expect(message).toContain('- CONTRACT_ID:');
      expect(message).toContain('- RPC_URL:');
    }
  });
});
