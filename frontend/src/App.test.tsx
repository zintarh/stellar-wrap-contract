import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import App from './App';

/**
 * Security review assertions for issue #846.
 *
 * The frontend must never persist wallet material (private keys, seeds,
 * signed payloads) to browser storage, and must treat RPC/contract
 * responses as untrusted input.
 */

const WALLET_MATERIAL_PATTERNS = [
  /private[_-]?key/i,
  /secret[_-]?key/i,
  /seed[_-]?phrase/i,
  /mnemonic/i,
  /0x[0-9a-f]{64}/i, // raw 32-byte hex (private key / signature)
];

function storageContainsWalletMaterial(storage: Storage): boolean {
  for (let i = 0; i < storage.length; i += 1) {
    const key = storage.key(i);
    if (key === null) continue;
    const value = storage.getItem(key) ?? '';
    const haystack = `${key}=${value}`;
    if (WALLET_MATERIAL_PATTERNS.some((pattern) => pattern.test(haystack))) {
      return true;
    }
  }
  return false;
}

function cookieContainsWalletMaterial(): boolean {
  const cookies = document.cookie ?? '';
  return WALLET_MATERIAL_PATTERNS.some((pattern) => pattern.test(cookies));
}

describe('wallet data handling (#846)', () => {
  beforeEach(() => {
    window.localStorage.clear();
    window.sessionStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('never persists wallet material to localStorage, sessionStorage, or cookies', async () => {
    render(<App />);

    // Allow any async wallet/RPC initialization to settle.
    await waitFor(() => {
      expect(document.body).toBeTruthy();
    });

    expect(storageContainsWalletMaterial(window.localStorage)).toBe(false);
    expect(storageContainsWalletMaterial(window.sessionStorage)).toBe(false);
    expect(cookieContainsWalletMaterial()).toBe(false);
  });

  it('does not log sensitive wallet material to the console', async () => {
    const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const infoSpy = vi.spyOn(console, 'info').mockImplementation(() => {});
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(<App />);

    await waitFor(() => {
      expect(document.body).toBeTruthy();
    });

    const allCalls = [
      ...logSpy.mock.calls,
      ...infoSpy.mock.calls,
      ...warnSpy.mock.calls,
      ...errorSpy.mock.calls,
    ];

    for (const call of allCalls) {
      const serialized = call
        .map((arg) => {
          if (typeof arg === 'string') return arg;
          try {
            return JSON.stringify(arg);
          } catch {
            return String(arg);
          }
        })
        .join(' ');

      for (const pattern of WALLET_MATERIAL_PATTERNS) {
        expect(serialized).not.toMatch(pattern);
      }
    }
  });

  it('renders contract-supplied strings as text, not HTML', async () => {
    const malicious = '<img src=x onerror="window.__xss=1">';
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ name: malicious, symbol: malicious }),
    } as unknown as Response);

    render(<App />);

    await waitFor(() => {
      expect(document.body).toBeTruthy();
    });

    // The injected payload must never become a live DOM node.
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect((window as unknown as { __xss?: number }).__xss).toBeUndefined();

    fetchSpy.mockRestore();
  });
});
