import { describe, expect, it } from 'vitest';
import {
  groupTtlRenewalBatches,
  reserveTransactionFee,
  selectTtlRenewals,
} from '../renewer';

describe('selectTtlRenewals', () => {
  it('selects live records inside the configured horizon and omits missing or expired records', () => {
    const records = [
      { user: 'GUSER1', period: 202501 },
      { user: 'GUSER1', period: 202502 },
      { user: 'GUSER2', period: 202501 },
      { user: 'GUSER2', period: 202502 },
    ];
    const liveUntil = new Map([
      ['GUSER1/202501', 100_000],
      ['GUSER1/202502', 1_000_000],
      ['GUSER2/202501', 1],
    ]);

    const result = selectTtlRenewals(records, liveUntil, 10, 30);

    expect(result).toEqual({
      candidates: [
        { user: 'GUSER1', period: 202501, liveUntilLedgerSeq: 100_000 },
      ],
      missing: 1,
      expired: 1,
    });
  });

  it('rejects an invalid renewal horizon', () => {
    expect(() => selectTtlRenewals([], new Map(), 10, 0)).toThrow(
      'TTL renewal threshold must be a positive whole number of days',
    );
  });

  it('groups records by user and respects the contract batch limit', () => {
    const candidates = Array.from({ length: 102 }, (_, index) => ({
      user: 'GUSER1',
      period: 202001 + index,
      liveUntilLedgerSeq: 100_000,
    }));
    candidates.push({
      user: 'GUSER2',
      period: 202501,
      liveUntilLedgerSeq: 100_000,
    });

    expect(groupTtlRenewalBatches(candidates)).toEqual([
      {
        user: 'GUSER1',
        periods: candidates.slice(0, 100).map(({ period }) => period),
      },
      {
        user: 'GUSER1',
        periods: candidates.slice(100, 102).map(({ period }) => period),
      },
      { user: 'GUSER2', periods: [202501] },
    ]);
  });

  it('prevents cumulative simulated transaction fees from exceeding the run cap', () => {
    expect(reserveTransactionFee(40n, 60n, 100n)).toBe(100n);
    expect(() => reserveTransactionFee(40n, 61n, 100n)).toThrow(
      'TTL renewal would exceed spending cap',
    );
  });
});
