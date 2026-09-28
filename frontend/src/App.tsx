import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import {
  errorMessage,
  formatPeriod,
  formatTimestamp,
  parseHexBytes,
  shortAddress,
  validatePeriod,
} from "./lib/format";
import { connectWallet } from "./lib/freighter";
import { getWrap, loadDashboard, mintWrap, validateConfig } from "./lib/stellar";
import type {
  Dashboard,
  NetworkConfig,
  WalletSession,
  WrapRecord,
} from "./lib/types";

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";

type BusyAction = "connect" | "refresh" | "search" | "mint" | null;

type RecordState = "active" | "revoked" | "burned" | "expired" | "opted-out";

const STATE_LABELS: Record<RecordState, string> = {
  active: "Active",
  revoked: "Revoked",
  burned: "Burned",
  expired: "Expired",
  "opted-out": "Opted out",
};

const STATE_DESCRIPTIONS: Record<RecordState, string> = {
  active: "Valid soulbound wrap record held by this account.",
  revoked: "Revoked by the issuer; no longer valid.",
  burned: "Burned by the holder; permanently destroyed.",
  expired: "Past its validity period; no longer active.",
  "opted-out": "Holder opted out of this record.",
};

function resolveRecordState(record: WrapRecord): RecordState {
  if (record.revoked) {
    return "revoked";
  }
  if (record.burned) {
    return "burned";
  }
  if (record.optedOut) {
    return "opted-out";
  }
  if (record.expired) {
    return "expired";
  }
  return "active";
}

const initialDraft: NetworkConfig = {
  contractId: import.meta.env.VITE_STELLAR_CONTRACT_ID ?? "",
  rpcUrl: import.meta.env.VITE_STELLAR_RPC_URL ?? DEFAULT_RPC_URL,
  networkPassphrase:
    import.meta.env.VITE_STELLAR_NETWORK_PASSPHRASE ?? TESTNET_PASSPHRASE,
};

function WrapCard({
  record,
  title,
}: {
  record: WrapRecord;
  title: string;
}) {
  const state = resolveRecordState(record);
  return (
    <article
      className={`wrap-card wrap-card--${state}`}
      aria-label={`${title}: ${record.archetype}, ${STATE_LABELS[state]}`}
    >
      <div className="wrap-card__heading">
        <div>
          <span className="eyebrow">{title}</span>
          <h3>{record.archetype}</h3>
        </div>
        <div className="wrap-card__badges">
          <span
            className={`state-badge state-badge--${state}`}
            title={STATE_DESCRIPTIONS[state]}
          >
            {STATE_LABELS[state]}
          </span>
          <span className="period-pill" title={`Raw period: ${record.period}`}>
            {formatPeriod(record.period)}
          </span>
        </div>
      </div>
      <p className="wrap-card__state-note">{STATE_DESCRIPTIONS[state]}</p>
      <dl className="record-grid">
        <div>
          <dt>Minted</dt>
          <dd>{formatTimestamp(record.timestamp)}</dd>
        </div>
        <div>
          <dt>Data hash</dt>
          <dd className="hash-value" title={record.dataHash}>
            {record.dataHash}
          </dd>
        </div>
      </dl>
      <p className="wrap-card__soulbound">
        Soulbound — this record cannot be transferred or sold.
      </p>
    </article>
  );
}

function Field({
  id,
  label,
  hint,
  children,
}: {
  id: string;
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children}
      {hint ? <span className="field__hint">{hint}</span> : null}
    </div>
  );
}

export default function App() {
  const [draft, setDraft] = useState(initialDraft);
  const [config, setConfig] = useState<NetworkConfig | null>(null);
  const [wallet, setWallet] = useState<WalletSession | null>(null);
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [searchPeriod, setSearchPeriod] = useState("");
  const [searchResult, setSearchResult] = useState<
    WrapRecord | null | undefined
  >(undefined);
  const [mintPeriod, setMintPeriod] = useState("");
  const [archetype, setArchetype] = useState("");
  const [dataHash, setDataHash] = useState("");
  const [signature, setSignature] = useState("");
  const [transactionHash, setTransactionHash] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState<BusyAction>(null);

  const connectButtonRef = useRef<HTMLButtonElement | null>(null);
  const wasConnectedRef = useRef(false);

  const clearMessages = () => {
    setError("");
    setNotice("");
  };

  const refresh = useCallback(
    async (
      activeConfig: NetworkConfig | null = config,
      activeWallet: WalletSession | null = wallet,
    ) => {
      if (!activeConfig || !activeWallet) {
        return;
      }
      if (
        activeWallet.networkPassphrase !== activeConfig.networkPassphrase
      ) {
        throw new Error(
          `Freighter is on ${activeWallet.network}. Switch it to the configured network and reconnect.`,
        );
      }

      setDashboard(await loadDashboard(activeConfig, activeWallet.address));
    },
    [config, wallet],
  );

  // Manage focus when the wallet connection state changes: when a wallet
  // connects, move focus to the connected status region; when it disconnects
  // or errors, return focus to the wallet-connect control so keyboard users
  // are not dropped back at the top of the document.
  useEffect(() => {
    if (wallet && !wasConnectedRef.current) {
      wasConnectedRef.current = true;
      document.getElementById("wallet-status")?.focus();
    } else if (!wallet && wasConnectedRef.current) {
      wasConnectedRef.current = false;
      connectButtonRef.current?.focus();
    }
  }, [wallet]);

  const handleConfigure = (event: FormEvent) => {
    event.preventDefault();
    clearMessages();
    try {
      const nextConfig = validateConfig(draft);
      setConfig(nextConfig);
      setDashboard(null);
      setSearchResult(undefined);
      setTransactionHash("");
      setNotice("Contract configuration applied. Connect Freighter to continue.");
    } catch (cause) {
      setError(errorMessage(cause));
    }
  };

  const handleConnect = async () => {
    if (!config) {
      return;
    }
    clearMessages();
    setBusy("connect");
    try {
      const nextWallet = await connectWallet();
      if (nextWallet.networkPassphrase !== config.networkPassphrase) {
        throw new Error(
          `Freighter is on ${nextWallet.network}. Switch it to the configured network and reconnect.`,
        );
      }
      setWallet(nextWallet);
      await refresh(config, nextWallet);
      setNotice("Freighter connected.");
    } catch (cause) {
      setWallet(null);
      setDashboard(null);
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const handleRefresh = async () => {
    clearMessages();
    setBusy("refresh");
    try {
      await refresh();
      setNotice("On-chain data refreshed.");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const handleSearch = async (event: FormEvent) => {
    event.preventDefault();
    if (!config || !wallet) {
      return;
    }
    clearMessages();
    setSearchResult(undefined);
    setBusy("search");
    try {
      const period = validatePeriod(searchPeriod);
      setSearchResult(await getWrap(config, wallet.address, period));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const handleMint = async (event: FormEvent) => {
    event.preventDefault();
    if (!config || !wallet) {
      return;
    }
    clearMessages();
    setTransactionHash("");
    setBusy("mint");
    try {
      const period = validatePeriod(mintPeriod);
      if (!/^[A-Za-z0-9_]{1,32}$/.test(archetype)) {
        throw new Error(
          "Archetype must be 1–32 letters, numbers, or underscores.",
        );
      }
      const hash = await mintWrap(config, wallet.address, {
        period,
        archetype,
        dataHash: parseHexBytes(dataHash, 32, "Data hash"),
        signature: parseHexBytes(signature, 64, "Admin signature"),
      });
      setTransactionHash(hash);
      setNotice("Wrap minted and confirmed on-chain.");
      await refresh(config, wallet);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const isBusy = busy !== null;

  const walletStatus = !wallet
    ? "Not connected"
    : wallet.networkPassphrase === config?.networkPassphrase
      ? `Connected as ${shortAddress(wallet.address)} on ${wallet.network}`
      : `Wrong network: Freighter is on ${wallet.network}`;

  return (
    <div className="app-shell">
      <a className="skip-link" href="#main-content">
        Skip to main content
      </a>
      <header className="topbar">
        <a className="brand" href="#top" aria-label="Stellar Wrap home">
          <span className="brand__mark" aria-hidden="true">
            W
          </span>
          <span>
            <strong>Stellar Wrap</strong>
            <small>On-chain registry</small>
          </span>
        </a>
        <div className="wallet-area">
          <p
            id="wallet-status"
            className="wallet-status"
            role="status"
            aria-live="polite"
            tabIndex={-1}
          >
            {walletStatus}
          </p>
          <button
            ref={connectButtonRef}
            type="button"
            className="button button--primary"
            onClick={handleConnect}
            disabled={!config || isBusy}
            aria-busy={busy === "connect"}
            aria-label={
              wallet
                ? "Reconnect Freighter wallet"
                : "Connect Freighter wallet"
            }
          >
            {busy === "connect"
              ? "Connecting…"
              : wallet
                ? "Reconnect wallet"
                : "Connect wallet"}
          </button>
        </div>
      </header>

      <main id="main-content" tabIndex={-1}>
        <div
          className="status-region"
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          {busy ? <p className="status status--busy">Loading: {busy}…</p> : null}
          {notice ? <p className="status status--notice">{notice}</p> : null}
          {error ? (
            <p className="status status--error" role="alert">
              Error: {error}
            </p>
          ) : null}
        </div>

        <section className="panel" aria-labelledby="config-heading">
          <h2 id="config-heading">Contract configuration</h2>
          <form onSubmit={handleConfigure}>
            <Field id="contract-id" label="Contract ID">
              <input
                id="contract-id"
                value={draft.contractId}
                onChange={(event) =>
                  setDraft({ ...draft, contractId: event.target.value })
                }
                required
              />
            </Field>
            <Field id="rpc-url" label="RPC URL">
              <input
                id="rpc-url"
                value={draft.rpcUrl}
                onChange={(event) =>
                  setDraft({ ...draft, rpcUrl: event.target.value })
                }
                required
              />
            </Field>
            <Field id="network-passphrase" label="Network passphrase">
              <input
                id="network-passphrase"
                value={draft.networkPassphrase}
                onChange={(event) =>
                  setDraft({ ...draft, networkPassphrase: event.target.value })
                }
                required
              />
            </Field>
            <button type="submit" className="button">
              Apply configuration
            </button>
          </form>
        </section>

        {wallet ? (
          <section className="panel" aria-labelledby="records-heading">
            <div className="panel__heading">
              <h2 id="records-heading">Your records</h2>
              <button
                type="button"
                className="button"
                onClick={handleRefresh}
                disabled={isBusy}
                aria-busy={busy === "refresh"}
              >
                {busy === "refresh" ? "Refreshing…" : "Refresh"}
              </button>
            </div>
            {dashboard ? (
              <div className="records">
                {dashboard.records.length === 0 ? (
                  <p>No wrap records found for this account.</p>
                ) : (
                  dashboard.records.map((record) => (
                    <WrapCard
                      key={record.period}
                      record={record}
                      title="Held record"
                    />
                  ))
                )}
              </div>
            ) : (
              <p>Loading records…</p>
            )}
          </section>
        ) : null}

        {wallet ? (
          <section className="panel" aria-labelledby="search-heading">
            <h2 id="search-heading">Verify a record</h2>
            <form onSubmit={handleSearch}>
              <Field id="search-period" label="Period">
                <input
                  id="search-period"
                  value={searchPeriod}
                  onChange={(event) => setSearchPeriod(event.target.value)}
                  required
                />
              </Field>
              <button
                type="submit"
                className="button"
                disabled={isBusy}
                aria-busy={busy === "search"}
              >
                {busy === "search" ? "Verifying…" : "Verify record"}
              </button>
            </form>
            {searchResult === null ? (
              <p role="status">No record found for that period.</p>
            ) : searchResult ? (
              <WrapCard record={searchResult} title="Verified record" />
            ) : null}
          </section>
        ) : null}

        {wallet ? (
          <section className="panel" aria-labelledby="mint-heading">
            <h2 id="mint-heading">Mint a wrap</h2>
            <form onSubmit={handleMint}>
              <Field id="mint-period" label="Period">
                <input
                  id="mint-period"
                  value={mintPeriod}
                  onChange={(event) => setMintPeriod(event.target.value)}
                  required
                />
              </Field>
              <Field id="mint-archetype" label="Archetype">
                <input
                  id="mint-archetype"
                  value={archetype}
                  onChange={(event) => setArchetype(event.target.value)}
                  required
                />
              </Field>
              <Field id="mint-data-hash" label="Data hash">
                <input
                  id="mint-data-hash"
                  value={dataHash}
                  onChange={(event) => setDataHash(event.target.value)}
                  required
                />
              </Field>
              <Field id="mint-signature" label="Admin signature">
                <input
                  id="mint-signature"
                  value={signature}
                  onChange={(event) => setSignature(event.target.value)}
                  required
                />
              </Field>
              <button
                type="submit"
                className="button button--primary"
                disabled={isBusy}
                aria-busy={busy === "mint"}
              >
                {busy === "mint" ? "Minting…" : "Mint wrap"}
              </button>
            </form>
            {transactionHash ? (
              <p role="status" className="hash-value">
                Transaction confirmed: {transactionHash}
              </p>
            ) : null}
          </section>
        ) : null}
      </main>
    </div>
  );
}
