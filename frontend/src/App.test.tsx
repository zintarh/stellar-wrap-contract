import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import App from './App';
import * as freighter from './lib/freighter';
import * as stellar from './lib/stellar';

/**
 * Security review assertions for issue #846.
 *
 * The frontend must never persist wallet material (private keys, seeds,
 * signed payloads) to browser storage, and must treat RPC/contract
 * responses as untrusted input.
 */

/**
 * A valid startup configuration (issue #839): the app is booted with a
 * validated environment instead of reading `import.meta.env` itself.
 */
const INITIAL_CONFIG = {
  contractId: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC',
  rpcUrl: 'https://soroban-testnet.stellar.org',
  networkPassphrase: 'Test SDF Network ; September 2015',
  networkName: 'Testnet',
};

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
    render(<App initialConfig={INITIAL_CONFIG} />);

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

    render(<App initialConfig={INITIAL_CONFIG} />);

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

    render(<App initialConfig={INITIAL_CONFIG} />);

    await waitFor(() => {
      expect(document.body).toBeTruthy();
    });

    // The injected payload must never become a live DOM node.
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect((window as unknown as { __xss?: number }).__xss).toBeUndefined();

    fetchSpy.mockRestore();
  });
});

describe('App UI states', () => {
  let connectSpy: ReturnType<typeof vi.spyOn>;
  let dashboardSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    connectSpy = vi.spyOn(freighter, 'connectWallet');
    dashboardSpy = vi.spyOn(stellar, 'loadDashboard');
    vi.spyOn(stellar, 'validateConfig').mockImplementation((c) => c);
  });
  
  afterEach(() => {
    vi.restoreAllMocks();
  });
  
  async function setupAndConnect() {
    render(<App initialConfig={INITIAL_CONFIG} />);
    // Apply config
    fireEvent.click(screen.getByRole('button', { name: /Apply configuration/i }));
    
    // Wait for connect button to be enabled
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Connect (Freighter )?wallet/i })).not.toBeDisabled();
    });
    
    // Click connect
    fireEvent.click(screen.getByRole('button', { name: /Connect (Freighter )?wallet/i }));
  }

  it('renders an error when no wallet is installed', async () => {
    connectSpy.mockRejectedValue(new Error("Freighter was not detected. Install or unlock the extension, then try again."));
    await setupAndConnect();
    
    expect(await screen.findByRole('alert')).toHaveTextContent("Error: Freighter was not detected. Install or unlock the extension, then try again.");
  });

  it('renders an error when wallet is installed but locked', async () => {
    connectSpy.mockRejectedValue(new Error("Wallet access was not approved."));
    await setupAndConnect();
    
    expect(await screen.findByRole('alert')).toHaveTextContent("Error: Wallet access was not approved.");
  });

  it('renders an error when wallet is connected on the wrong network', async () => {
    connectSpy.mockResolvedValue({
      address: "G123",
      network: "PUBLIC",
      networkPassphrase: "Public Global Stellar Network ; September 2015"
    });
    await setupAndConnect();
    
    // "Wrong network" error is thrown by the check in handleConnect
    expect(await screen.findByRole('alert')).toHaveTextContent("Switch it to the configured network and reconnect.");
  });

  it('renders an empty state when connected with no wrap records', async () => {
    connectSpy.mockResolvedValue({
      address: "G123",
      network: "TESTNET",
      networkPassphrase: "Test SDF Network ; September 2015"
    });
    dashboardSpy.mockResolvedValue({ records: [], totalCount: 0 });
    
    await setupAndConnect();
    
    expect(await screen.findByText("No wrap records found for this account.")).toBeInTheDocument();
  });

  it('renders records when connected with records', async () => {
    connectSpy.mockResolvedValue({
      address: "G123",
      network: "TESTNET",
      networkPassphrase: "Test SDF Network ; September 2015"
    });
    dashboardSpy.mockResolvedValue({
      records: [
        {
          period: 202401,
          archetype: "builder",
          dataHash: "hash123",
          createdAt: 1234567n,
          revoked: false,
          burned: false,
          expired: false,
          optedOut: false,
        }
      ],
      totalCount: 1,
    });
    
    await setupAndConnect();
    
    expect(await screen.findByText("builder")).toBeInTheDocument();
  });

  it('differentiates loading state from empty state', async () => {
    connectSpy.mockResolvedValue({
      address: "G123",
      network: "TESTNET",
      networkPassphrase: "Test SDF Network ; September 2015"
    });
    
    let resolveDashboard: (value: {
      records: unknown[];
      totalCount: number;
    }) => void = () => {};
    const dashboardPromise = new Promise<{
      records: unknown[];
      totalCount: number;
    }>((resolve) => {
      resolveDashboard = resolve;
    });
    dashboardSpy.mockReturnValue(dashboardPromise);
    
    await setupAndConnect();
    
    // While loading, we should see "Loading records..." and not "No wrap records"
    expect(await screen.findByText("Loading records…")).toBeInTheDocument();
    expect(screen.queryByText("No wrap records found for this account.")).not.toBeInTheDocument();
    
    // Now resolve it
    resolveDashboard({ records: [], totalCount: 0 });
    
    // Now we should see the empty state
    expect(await screen.findByText("No wrap records found for this account.")).toBeInTheDocument();
    expect(screen.queryByText("Loading records…")).not.toBeInTheDocument();
  });
  
  it('renders a failed contract call as an actionable error rather than an empty state', async () => {
    connectSpy.mockResolvedValue({
      address: "G123",
      network: "TESTNET",
      networkPassphrase: "Test SDF Network ; September 2015"
    });
    dashboardSpy.mockRejectedValue(new Error("RPC node timeout"));
    
    await setupAndConnect();
    
    expect(await screen.findByRole('alert')).toHaveTextContent("Error: RPC node timeout");
    expect(screen.queryByText("No wrap records found for this account.")).not.toBeInTheDocument();
  });
});

describe('Visible network configuration (#839)', () => {
  it('shows the configured network, contract id, and RPC endpoint', async () => {
    render(<App initialConfig={INITIAL_CONFIG} />);

    const summary = screen.getByTestId('config-summary');
    expect(summary).toHaveTextContent('Network: Testnet');
    expect(summary).toHaveTextContent('RPC: soroban-testnet.stellar.org');
    expect(summary.querySelector('.hash-value')).toHaveAttribute(
      'title',
      INITIAL_CONFIG.contractId,
    );
    expect(
      summary.querySelector(
        '[title="Test SDF Network ; September 2015"]',
      ),
    ).not.toBeNull();
  });

  it('updates the visible network when a different configuration is applied', async () => {
    render(<App initialConfig={INITIAL_CONFIG} />);

    fireEvent.change(screen.getByLabelText('Network passphrase'), {
      target: { value: 'Public Global Stellar Network ; September 2015' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Apply configuration/i }));

    expect(screen.getByTestId('config-summary')).toHaveTextContent(
      'Network: Public',
    );
  });
});
