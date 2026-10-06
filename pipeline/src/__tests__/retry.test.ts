import { describe, it, expect, vi } from 'vitest';
import { backoffDelayMs, isTransientRpcError, withRetries } from '../retry';

function netErr(code: string, message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code });
}

function httpErr(status: number): Error & { response: { status: number } } {
  const err = Object.assign(new Error(`http ${status}`), {
    response: { status },
  });
  return err;
}

describe('isTransientRpcError', () => {
  it('treats network-level failures as transient', () => {
    expect(isTransientRpcError(netErr('ECONNREFUSED', 'connect'))).toBe(true);
    expect(isTransientRpcError(netErr('ETIMEDOUT', 'timeout'))).toBe(true);
    expect(isTransientRpcError(netErr('ECONNRESET', 'socket hang up'))).toBe(true);
    expect(isTransientRpcError(netErr('EAI_AGAIN', 'getaddrinfo'))).toBe(true);
  });

  it('treats 429, 408, 425 and 5xx as transient', () => {
    expect(isTransientRpcError(httpErr(429))).toBe(true);
    expect(isTransientRpcError(httpErr(408))).toBe(true);
    expect(isTransientRpcError(httpErr(425))).toBe(true);
    expect(isTransientRpcError(httpErr(500))).toBe(true);
    expect(isTransientRpcError(httpErr(503))).toBe(true);
  });

  it('treats other 4xx responses as permanent', () => {
    expect(isTransientRpcError(httpErr(400))).toBe(false);
    expect(isTransientRpcError(httpErr(404))).toBe(false);
    expect(isTransientRpcError(httpErr(403))).toBe(false);
  });

  it('treats JSON-RPC protocol errors (invalid contract id) as permanent', () => {
    expect(isTransientRpcError({ code: -32602, message: 'Invalid contractID' })).toBe(false);
    expect(isTransientRpcError({ code: -32601, message: 'Method not found' })).toBe(false);
    expect(isTransientRpcError({ code: -32700, message: 'Parse error' })).toBe(false);
  });
});

describe('backoffDelayMs', () => {
  it('grows exponentially and caps at maxDelayMs with zero jitter', () => {
    expect(backoffDelayMs(0, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(100);
    expect(backoffDelayMs(1, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(200);
    expect(backoffDelayMs(2, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(400);
    expect(backoffDelayMs(3, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(800);
    expect(backoffDelayMs(4, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(1000);
    expect(backoffDelayMs(9, { baseDelayMs: 100, maxDelayMs: 1000, jitter: 0 })).toBe(1000);
  });

  it('keeps delays within the jitter bounds', () => {
    for (let i = 0; i < 200; i += 1) {
      const delay = backoffDelayMs(0, { baseDelayMs: 1000, maxDelayMs: 10000, jitter: 0.2 });
      expect(delay).toBeGreaterThanOrEqual(800);
      expect(delay).toBeLessThanOrEqual(1200);
    }
  });
});

describe('withRetries', () => {
  it('retries transient failures and logs entry into and exit from degraded state', async () => {
    const fn = vi
      .fn()
      .mockRejectedValueOnce(netErr('ECONNREFUSED', 'connect'))
      .mockRejectedValueOnce(netErr('ETIMEDOUT', 'timeout'))
      .mockResolvedValueOnce('ok');
    const log = vi.fn();

    await expect(
      withRetries('fetchEvents', fn, { maxRetries: 5, sleep: async () => {}, log }),
    ).resolves.toBe('ok');

    expect(fn).toHaveBeenCalledTimes(3);
    expect(log).toHaveBeenCalledWith(expect.stringContaining('entering degraded state'));
    expect(log).toHaveBeenCalledWith(expect.stringContaining('recovered after 2 retries'));
  });

  it('gives up after maxRetries and rethrows the last error', async () => {
    const err = netErr('ETIMEDOUT', 'down');
    const fn = vi.fn().mockRejectedValue(err);
    const log = vi.fn();

    await expect(
      withRetries('fetchEvents', fn, { maxRetries: 2, sleep: async () => {}, log }),
    ).rejects.toBe(err);

    expect(fn).toHaveBeenCalledTimes(3);
    expect(log).toHaveBeenCalledWith(expect.stringContaining('giving up after 2 retries'));
  });

  it('does not retry permanent errors', async () => {
    const err = { code: -32602, message: 'Invalid contractID' };
    const fn = vi.fn().mockRejectedValue(err);

    await expect(
      withRetries('fetchEvents', fn, { maxRetries: 5, sleep: async () => {} }),
    ).rejects.toBe(err);

    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('applies increasing backoff between retries', async () => {
    const delays: number[] = [];
    const sleep = async (ms: number) => {
      delays.push(ms);
    };
    const fn = vi
      .fn()
      .mockRejectedValueOnce(netErr('ECONNREFUSED', 'a'))
      .mockRejectedValueOnce(netErr('ECONNREFUSED', 'b'))
      .mockRejectedValueOnce(netErr('ECONNREFUSED', 'c'))
      .mockResolvedValueOnce('ok');

    await withRetries('x', fn, {
      maxRetries: 5,
      baseDelayMs: 100,
      maxDelayMs: 10000,
      jitter: 0,
      sleep,
      log: () => {},
    });

    expect(delays).toHaveLength(3);
    expect(delays[0]).toBe(100);
    expect(delays[1]).toBe(200);
    expect(delays[2]).toBe(400);
  });
});