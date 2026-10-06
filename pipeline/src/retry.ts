/**
 * Bounded retry with exponential backoff + jitter for Soroban RPC calls.
 *
 * The indexer's only source of truth is a remote RPC endpoint that rate-limits,
 * times out, and returns partial pages under load. This module gives the fetcher
 * a single, testable policy: retry transient failures with backoff, never touch
 * permanent ones (an invalid contract id must fail fast, not loop forever), and
 * bound the whole thing so a dead endpoint cannot spin the process.
 */

export interface RetryConfig {
  /** Number of times to retry before giving up. */
  maxRetries: number;
  /** Delay (ms) before the first retry; doubles each attempt. */
  baseDelayMs: number;
  /** Upper bound (ms) on the backoff delay. */
  maxDelayMs: number;
  /** Jitter ratio (0..1) applied to each computed delay. */
  jitter: number;
}

export const DEFAULT_RETRY: Required<RetryConfig> = {
  maxRetries: 5,
  baseDelayMs: 500,
  maxDelayMs: 30_000,
  jitter: 0.25,
};

export interface ErrorLike {
  response?: { status?: number };
  code?: string | number;
  message?: string;
}

/**
 * Classify an error thrown by the Stellar SDK as transient (safe to retry) or
 * permanent (retrying will always fail identically).
 *
 * - A JSON-RPC protocol error (numeric `code`, e.g. invalid params / unknown
 *   contract id) is permanent — the request itself was rejected.
 * - An HTTP 4xx status (except 408/425/429) is a permanent client error.
 * - Rate limits (429), server errors (5xx), and HTTP-level 408/425 are transient.
 * - Network-level failures (DNS, connect, reset, timeout) are transient.
 * - Anything else is treated as transient: the indexer must not drop ledger data
 *   because of an unrecognized error, and the retry loop is bounded regardless.
 */
export function isTransientRpcError(err: unknown): boolean {
  const e = (err ?? {}) as ErrorLike;

  if (typeof e.code === 'number') {
    return false;
  }

  if (typeof e.response?.status === 'number') {
    const status = e.response.status;
    if (status === 408 || status === 425 || status === 429) return true;
    if (status >= 500) return true;
    return false;
  }

  const code = typeof e.code === 'string' ? e.code.toUpperCase() : '';
  const msg = (e.message ?? '').toLowerCase();
  if (
    /^ECONNRESET$|^ECONNABORTED$|^ETIMEDOUT$|^ECONNREFUSED$|^EAI_AGAIN$|^ENOTFOUND$|^ERR_NETWORK$|^UND_ERR/
      .test(code)
  ) {
    return true;
  }
  if (
    /timeout|econnreset|socket hang up|network error|fetch failed|etimedout|econnrefused|eai_again/
      .test(msg)
  ) {
    return true;
  }
  return true;
}

/**
 * Compute the delay for a retry: exponential backoff capped at `maxDelayMs`,
 * scaled by a jitter ratio so synchronized retry storms don't re-collide.
 */
export function backoffDelayMs(
  retryCount: number,
  cfg: Partial<RetryConfig> = {},
): number {
  const baseDelayMs = cfg.baseDelayMs ?? DEFAULT_RETRY.baseDelayMs;
  const maxDelayMs = cfg.maxDelayMs ?? DEFAULT_RETRY.maxDelayMs;
  const jitter = Math.min(cfg.jitter ?? DEFAULT_RETRY.jitter, 1);
  const exponential = Math.min(maxDelayMs, baseDelayMs * Math.pow(2, retryCount));
  const spread = exponential * jitter;
  const min = Math.max(0, exponential - spread);
  const max = exponential + spread;
  return Math.round(min + Math.random() * (max - min));
}

export interface WithRetriesOptions extends Partial<RetryConfig> {
  /** Override the transient/permanent classifier. */
  shouldRetry?: (err: unknown) => boolean;
  /** Emit degraded-state log lines (defaults to `console.log`). */
  log?: (msg: string) => void;
  /** Delay implementation; injectable for tests. */
  sleep?: (ms: number) => Promise<void>;
}

function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'string') return err;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

/**
 * Run `fn`, retrying transient failures with exponential backoff and jitter
 * until `maxRetries` is exhausted. Logs entry into and exit from a degraded
 * state (the first failure and the recovery), not just each individual retry.
 * Permanent errors are rethrown immediately without a retry.
 */
export async function withRetries<T>(
  operation: string,
  fn: () => Promise<T>,
  opts: WithRetriesOptions = {},
): Promise<T> {
  const maxRetries = opts.maxRetries ?? DEFAULT_RETRY.maxRetries;
  const shouldRetry = opts.shouldRetry ?? isTransientRpcError;
  const log = opts.log ?? ((msg: string) => console.log(msg));
  const sleep =
    opts.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));

  let retries = 0;
  let degraded = false;

  for (;;) {
    try {
      const result = await fn();
      if (degraded) {
        log(`[rpc] ${operation}: recovered after ${retries} retries`);
      }
      return result;
    } catch (err) {
      if (!shouldRetry(err)) {
        throw err;
      }
      if (retries >= maxRetries) {
        if (degraded) {
          log(`[rpc] ${operation}: giving up after ${maxRetries} retries: ${errorMessage(err)}`);
        }
        throw err;
      }
      const delayMs = backoffDelayMs(retries, opts);
      if (!degraded) {
        degraded = true;
        log(
          `[rpc] ${operation}: entering degraded state (${errorMessage(err)}); ` +
            `retrying up to ${maxRetries} times`,
        );
      } else {
        log(
          `[rpc] ${operation}: retry ${retries}/${maxRetries} failed (${errorMessage(err)}); ` +
            `next attempt in ${delayMs}ms`,
        );
      }
      await sleep(delayMs);
      retries += 1;
    }
  }
}