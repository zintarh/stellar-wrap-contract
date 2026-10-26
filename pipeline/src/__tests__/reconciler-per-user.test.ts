import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { IndexerDB } from '../db';
import { reconcile } from '../reconciler';
import type { PerUserMismatch } from '../reconciler';
import { DataKeyVariant, WrapState, type WrapRecord } from '../types';

/**
 * Tests for issue #1006: the reconciler must compare per-user state, not only
 * global counters. Per-user drift — a wrap the indexer missed — is invisible
 * as long as the global totals happen to match, yet it is exactly what users
 * see in their own history.
 */

const contractId = 'CCONTRACT';

function wrapRecord(
  period: number,
  overrides: Partial<WrapRecord> = {},
): WrapRecord {
  return {
    created_at: 1000 + period,
    data_hash: `hash-${period}`,
    archetype: 'default',
    period,
    lifecycle: { state: WrapState.Active, updated_at: 2000 + period },
    ...overrides,
  };
}

function wrapEntry(user: string, record: WrapRecord, ledger = 300) {
  return {
    key: { variant: DataKeyVariant.Wrap, user, period: record.period },
    value: { type: 'wrap_record' as const, value: record },
    ledger,
    durability: 'persistent' as const,
  };
}

function upsertIndexed(
  db: IndexerDB,
  user: string,
  record: WrapRecord,
  ledger = 300,
  txHash = `tx-${user}-${record.period}`,
): void {
  db.upsertWrap({
    contract_id: contractId,
    user,
    period: record.period,
    timestamp: record.created_at,
    data_hash: record.data_hash,
    archetype: record.archetype,
    fsm_state: record.lifecycle.state,
    fsm_updated_at: record.lifecycle.updated_at,
    ledger_seq: ledger,
    tx_hash: txHash,
  });
}

/**
 * A fetcher serving the given on-chain storage, optionally with targeted
 * re-fetch support so the repair path can be exercised end to end.
 */
function mockFetcher(
  entries: unknown[],
  opts: { withKeyFetch?: boolean; keyFetchResult?: unknown[] } = {},
): any {
  const fetcher: any = {
    fetchStorageEntries: async () => entries,
  };
  if (opts.withKeyFetch) {
    fetcher.buildWrapKey = (user: string, period: number) => ({ user, period });
    fetcher.fetchStorageByKeys = async () => (opts.keyFetchResult ?? []);
  }
  return fetcher;
}

describe('reconcile (per-user sampling, issue #1006)', () => {
  let db: IndexerDB;

  beforeEach(async () => {
    db = await IndexerDB.create();
  });

  afterEach(() => {
    db.close();
  });

  /**
   * On-chain state. The global counter defaults to matching the indexed rows
   * so the global-only check would (wrongly) pass: per-user drift must still
   * show up.
   */
  function onChainEntries(
    users: { user: string; records: WrapRecord[] }[],
    totalWrapCount?: number,
  ): unknown[] {
    const entries: unknown[] = [
      {
        key: { variant: DataKeyVariant.Admin },
        value: { type: 'address', value: 'GADMIN' },
        ledger: 100,
        durability: 'instance',
      },
      {
        key: { variant: DataKeyVariant.TotalWrapCount },
        value: { type: 'u32', value: totalWrapCount ?? 2 },
        ledger: 300,
        durability: 'instance',
      },
    ];
    for (const { user, records } of users) {
      for (const record of records) entries.push(wrapEntry(user, record));
    }
    return entries;
  }

  function mismatchFor(
    mismatches: PerUserMismatch[],
    user: string,
    period: number,
  ): PerUserMismatch | undefined {
    return mismatches.find((m) => m.user === user && m.period === period);
  }

  it('reports per-user missing, extra and field_diff with user, period and kind', async () => {
    // Indexed: user GAU p1 (matches chain), user GBV p2 (data_hash drifts),
    // and user GCC p4 which exists only in the index (stale row).
    upsertIndexed(db, 'GAU', wrapRecord(1));
    upsertIndexed(db, 'GBV', wrapRecord(2, { data_hash: 'stale-hash' }));
    upsertIndexed(db, 'GCC', wrapRecord(4));

    const fetcher = mockFetcher(
      onChainEntries(
        [
          { user: 'GAU', records: [wrapRecord(1)] },
          { user: 'GBV', records: [wrapRecord(2)] },
          { user: 'GDW', records: [wrapRecord(3)] },
        ],
        3,
      ),
    );

    const report = await reconcile(db, fetcher, contractId);

    expect(report.mismatches).toHaveLength(0); // global counters match
    const mismatches = report.per_user.mismatches;
    expect(mismatches).toHaveLength(3);

    // missing: exists on-chain, absent from the index (the indexer missed it).
    const missing = mismatchFor(mismatches, 'GDW', 3);
    expect(missing?.kind).toBe('missing');
    expect(missing?.onchain?.data_hash).toBe('hash-3');

    // extra: exists only in the index (stale row).
    const extra = mismatchFor(mismatches, 'GCC', 4);
    expect(extra?.kind).toBe('extra');
    expect(extra?.indexed?.data_hash).toBe('hash-4');

    // field_diff: both sides exist but disagree, with the differing fields.
    const diff = mismatchFor(mismatches, 'GBV', 2);
    expect(diff?.kind).toBe('field_diff');
    expect(diff?.fields).toContain('data_hash');
    expect(diff?.indexed?.data_hash).toBe('stale-hash');
    expect(diff?.onchain?.data_hash).toBe('hash-2');

    expect(report.is_consistent).toBe(false);
  });

  it('counts per-user drift as its own alertable counter', async () => {
    upsertIndexed(db, 'GAU', wrapRecord(1));
    const fetcher = mockFetcher(
      onChainEntries([{ user: 'GAU', records: [wrapRecord(1), wrapRecord(2)] }]),
    );
    const report = await reconcile(db, fetcher, contractId);
    expect(report.per_user.mismatches).toHaveLength(1);
    expect(report.is_consistent).toBe(false);
  });

  it('samples a bounded mix of users instead of comparing everyone', async () => {
    // Three indexed users, none of which exist on-chain; each sampled user
    // yields exactly one 'extra' mismatch. random() = 0 keeps sampling
    // deterministic without depending on which user is picked.
    for (const user of ['GAU', 'GBV', 'GCC']) {
      upsertIndexed(db, user, wrapRecord(1));
    }
    const fetcher = mockFetcher(
      onChainEntries(
        [
          { user: 'GAU', records: [] },
          { user: 'GBV', records: [] },
          { user: 'GCC', records: [] },
        ],
        3,
      ),
    );
    const report = await reconcile(db, fetcher, contractId, {
      sampleUsers: 1,
      random: () => 0,
    });
    expect(report.per_user.sampled_users).toHaveLength(1);
    expect(report.per_user.mismatches).toHaveLength(1);
    expect(report.per_user.mismatches[0].kind).toBe('extra');
  });

  it('includes users only visible on-chain in the sample pool', async () => {
    // GAU exists in the index; GDW exists only on-chain. Sample 2 with the
    // recent-active budget covering GAU, so GDW must fill the other slot.
    upsertIndexed(db, 'GAU', wrapRecord(1));
    const fetcher = mockFetcher(
      onChainEntries([
        { user: 'GAU', records: [wrapRecord(1)] },
        { user: 'GDW', records: [wrapRecord(3)] },
      ]),
    );
    const report = await reconcile(db, fetcher, contractId, {
      sampleUsers: 2,
      random: () => 0,
    });
    expect(report.per_user.sampled_users).toContain('GAU');
    expect(report.per_user.sampled_users).toContain('GDW');
    const missing = mismatchFor(report.per_user.mismatches, 'GDW', 3);
    expect(missing?.kind).toBe('missing');
  });

  it('disables per-user comparison when sampleUsers is 0', async () => {
    upsertIndexed(db, 'GAU', wrapRecord(1));
    const fetcher = mockFetcher(
      onChainEntries([{ user: 'GDW', records: [wrapRecord(3)] }]),
    );
    const report = await reconcile(db, fetcher, contractId, { sampleUsers: 0 });
    expect(report.per_user.sampled_users).toHaveLength(0);
    expect(report.per_user.mismatches).toHaveLength(0);
  });

  describe('--repair', () => {
    it('re-fetches a stale extra row, confirms it is gone, and removes it', async () => {
      upsertIndexed(db, 'GAU', wrapRecord(1));
      // Chain really has nothing for GAU p1; the targeted re-fetch returns no
      // entry, confirming the indexed row is stale.
      const fetcher = mockFetcher(onChainEntries([]), {
        withKeyFetch: true,
        keyFetchResult: [],
      });
      const report = await reconcile(db, fetcher, contractId, { repair: true });
      expect(report.per_user.repaired).toBe(1);
      expect(report.per_user.mismatches).toHaveLength(0);
      expect(db.getWrapsForUser(contractId, 'GAU')).toHaveLength(0);
    });

    it('overwrites a drifted row with the re-fetched on-chain record', async () => {
      upsertIndexed(db, 'GAU', wrapRecord(1, { data_hash: 'stale-hash' }));
      const onChainRecord = wrapRecord(1, { data_hash: 'fresh-hash' });
      const fetcher = mockFetcher(onChainEntries([{ user: 'GAU', records: [onChainRecord] }]), {
        withKeyFetch: true,
        keyFetchResult: [wrapEntry('GAU', onChainRecord)],
      });
      const report = await reconcile(db, fetcher, contractId, { repair: true });
      expect(report.per_user.repaired).toBe(1);
      expect(report.per_user.mismatches).toHaveLength(0);
      const rows = db.getWrapsForUser(contractId, 'GAU');
      expect(rows).toHaveLength(1);
      expect(rows[0].data_hash).toBe('fresh-hash');
    });

    it('backfills a wrap the indexer missed (kind missing)', async () => {
      const onChainRecord = wrapRecord(3);
      const fetcher = mockFetcher([wrapEntry('GAU', onChainRecord)], {
        withKeyFetch: true,
        keyFetchResult: [wrapEntry('GAU', onChainRecord)],
      });
      const report = await reconcile(db, fetcher, contractId, { repair: true });
      expect(report.per_user.repaired).toBe(1);
      const rows = db.getWrapsForUser(contractId, 'GAU');
      expect(rows).toHaveLength(1);
      expect(rows[0].data_hash).toBe('hash-3');
      expect(rows[0].user).toBe('GAU');
    });

    it('falls back to the cached on-chain snapshot when targeted re-fetch is unavailable', async () => {
      const onChainRecord = wrapRecord(1, { data_hash: 'fresh-hash' });
      upsertIndexed(db, 'GAU', wrapRecord(1, { data_hash: 'stale-hash' }));
      const fetcher = mockFetcher(
        onChainEntries([{ user: 'GAU', records: [onChainRecord] }]),
      );
      const report = await reconcile(db, fetcher, contractId, { repair: true });
      expect(report.per_user.repaired).toBe(1);
      expect(db.getWrapsForUser(contractId, 'GAU')[0].data_hash).toBe('fresh-hash');
    });

    it('drops a missing mismatch the re-fetch shows was already resolved', async () => {
      // Snapshot said chain has GAU p1, but the fresh fetch finds nothing and
      // neither does the index: nothing to repair, nothing left to report.
      const fetcher = mockFetcher(
        onChainEntries([{ user: 'GAU', records: [wrapRecord(1)] }]),
        { withKeyFetch: true, keyFetchResult: [] },
      );
      upsertIndexed(db, 'GAU', wrapRecord(1));
      // Remove the indexed row after seeding so both sides are empty, making
      // the snapshot vs re-fetch disagreement explicit in the test setup.
      db.removeWrap(contractId, 'GAU', 1);

      const report = await reconcile(db, fetcher, contractId, { repair: true });
      expect(report.per_user.repaired).toBe(0);
      expect(report.per_user.mismatches).toHaveLength(0);
    });
  });
});
