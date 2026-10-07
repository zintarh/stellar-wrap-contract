import { describe, it, expect, vi } from 'vitest';
import { xdr, rpc, Contract } from '@stellar/stellar-sdk';
import { SorobanFetcher, type SorobanRpcClient } from '../fetcher';
import type { WithRetriesOptions } from '../retry';

const CONTRACT_ID = 'CC3GVOAVIKWIJPXZSAUZ545OCHUKGTJ7OKURU4PCUJWQM4MTZXU4VULZ';

function makeResponse(
  latestLedger: number,
  cursor: string,
  events: rpc.Api.EventResponse[],
): rpc.Api.GetEventsResponse {
  return { latestLedger, cursor, events };
}

function makeEvent(ledger: number, id = `event-${ledger}`): rpc.Api.EventResponse {
  return {
    type: 'contract',
    id,
    ledger,
    ledgerClosedAt: '2026-01-01T00:00:00Z',
    pagingToken: String(ledger),
    inSuccessfulContractCall: true,
    contractId: new Contract(CONTRACT_ID),
    txHash: 'tx',
    topic: [xdr.ScVal.scvSymbol('mint')],
    value: xdr.ScVal.scvSymbol('ok'),
  };
}

function netErr(code: string, message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code });
}

function makeFetcher(
  server: Partial<SorobanRpcClient>,
  retry?: Partial<WithRetriesOptions>,
): SorobanFetcher {
  return new SorobanFetcher({
    rpcUrl: 'https://rpc.example.com',
    contractId: CONTRACT_ID,
    eventPageSize: 1,
    server: {
      getEvents: async () => makeResponse(1, '', []),
      getLedgerEntries: async () => ({ entries: [], latestLedger: 1 }),
      getLatestLedger: async () => ({ id: 'id', sequence: 1, protocolVersion: 'x' }),
      ...server,
    },
    retry: {
      baseDelayMs: 1,
      maxDelayMs: 2,
      maxRetries: 5,
      sleep: async () => {},
      log: () => {},
      ...retry,
    },
  });
}

describe('SorobanFetcher retry behavior', () => {
  it('recovers from a transient RPC outage and returns caught-up events', async () => {
    const getEvents = vi
      .fn()
      .mockRejectedValueOnce(netErr('ETIMEDOUT', 'timeout'))
      .mockRejectedValueOnce(netErr('ECONNRESET', 'socket hang up'))
      .mockResolvedValueOnce(makeResponse(120, 'c120', [makeEvent(120)]))
      .mockResolvedValueOnce(makeResponse(120, 'c120', []));
    const log = vi.fn();
    const fetcher = makeFetcher({ getEvents }, { log });

    const result = await fetcher.fetchEvents(119);

    expect(getEvents).toHaveBeenCalledTimes(4);
    expect(result.latestLedger).toBe(120);
    expect(result.events).toHaveLength(1);
    expect(result.events[0].ledger).toBe(120);
    expect(log).toHaveBeenCalledWith(expect.stringContaining('entering degraded state'));
    expect(log).toHaveBeenCalledWith(expect.stringContaining('recovered after 2 retries'));
  });

  it('does not retry a permanent server error (invalid contract id)', async () => {
    const rpcError = { code: -32602, message: 'Invalid contractID' };
    const getEvents = vi.fn().mockRejectedValue(rpcError);
    const fetcher = makeFetcher({ getEvents });

    await expect(fetcher.fetchEvents(100)).rejects.toMatchObject({ code: -32602 });
    expect(getEvents).toHaveBeenCalledTimes(1);
  });

  it('throws on a partially-fetched page so the caller never advances the cursor', async () => {
    const getEvents = vi
      .fn()
      .mockResolvedValueOnce(makeResponse(120, 'c1', [makeEvent(120)]))
      .mockRejectedValueOnce({ code: -32602, message: 'bad request' });
    const fetcher = makeFetcher({ getEvents });

    await expect(fetcher.fetchEvents(100)).rejects.toMatchObject({ code: -32602 });

    const cursors = getEvents.mock.calls.map((call) => call[0].cursor);
    expect(cursors[0]).toBeUndefined();
    expect(cursors[1]).toBe('c1');
  });

  it('retries a mid-pagination outage with the same cursor and completes the batch', async () => {
    const getEvents = vi
      .fn()
      .mockResolvedValueOnce(makeResponse(120, 'c1', [makeEvent(120)]))
      .mockRejectedValueOnce(netErr('ECONNREFUSED', 'connect'))
      .mockResolvedValueOnce(makeResponse(121, 'c2', [makeEvent(121)]))
      .mockResolvedValueOnce(makeResponse(122, 'c2', []));
    const fetcher = makeFetcher({ getEvents });

    const result = await fetcher.fetchEvents(120);

    expect(result.events).toHaveLength(2);
    expect(result.events.map((e) => e.ledger)).toEqual([120, 121]);
    expect(result.latestLedger).toBe(122);

    const cursors = getEvents.mock.calls.map((call) => call[0].cursor);
    expect(cursors[0]).toBeUndefined();
    expect(cursors[1]).toBe('c1');
    expect(cursors[2]).toBe('c1');
    expect(cursors[3]).toBe('c2');
  });

  it('rethrows the last error after maxRetries during a sustained outage', async () => {
    const err = netErr('ETIMEDOUT', 'down');
    const getEvents = vi.fn().mockRejectedValue(err);
    const fetcher = makeFetcher({ getEvents }, { maxRetries: 2 });

    await expect(fetcher.fetchEvents(100)).rejects.toBe(err);
    expect(getEvents).toHaveBeenCalledTimes(3);
  });

  it('retries getLatestLedger on transient failure', async () => {
    const getLatestLedger = vi
      .fn()
      .mockRejectedValueOnce(
        Object.assign(new Error('rate limited'), { response: { status: 429 } }),
      )
      .mockResolvedValueOnce({ id: 'id', sequence: 500, protocolVersion: 'x' });
    const fetcher = makeFetcher({ getLatestLedger });

    await expect(fetcher.getLatestLedger()).resolves.toBe(500);
    expect(getLatestLedger).toHaveBeenCalledTimes(2);
  });
});