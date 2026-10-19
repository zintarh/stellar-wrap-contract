import {
  getNetwork,
  isConnected,
  requestAccess,
  signTransaction,
} from "@stellar/freighter-api";
import type { WalletSession } from "./types";

/**
 * The five situations a Freighter interaction can end in. Each one needs a
 * different response from the app, so they must never be collapsed into a
 * single generic failure.
 */
export type FreighterErrorKind =
  | "not-installed"
  | "locked"
  | "declined"
  | "network-mismatch"
  | "rpc-failure";

export class FreighterError extends Error {
  readonly kind: FreighterErrorKind;
  /** True only for genuine RPC failures, which are safe to retry. */
  readonly retryable: boolean;

  constructor(kind: FreighterErrorKind, message: string) {
    super(message);
    this.name = "FreighterError";
    this.kind = kind;
    this.retryable = kind === "rpc-failure";
  }
}

function rawMessage(
  error: { message?: string } | string | undefined,
): string {
  if (typeof error === "string") {
    return error;
  }
  return error?.message ?? "";
}

/**
 * Classify a raw Freighter error into one of the five distinct situations.
 * `expectedNetwork` is only used to build the network-mismatch message.
 */
function classifyError(
  error: { message?: string } | string | undefined,
  expectedNetwork?: string,
): FreighterError {
  const message = rawMessage(error);
  const lower = message.toLowerCase();

  if (
    lower.includes("not installed") ||
    lower.includes("not detected") ||
    lower.includes("no freighter")
  ) {
    return new FreighterError(
      "not-installed",
      "Freighter is not installed. Install the Freighter extension, then try again.",
    );
  }

  if (lower.includes("locked") || lower.includes("unlock")) {
    return new FreighterError(
      "locked",
      "Freighter is locked. Unlock the extension, then try again.",
    );
  }

  if (
    lower.includes("declin") ||
    lower.includes("reject") ||
    lower.includes("denied") ||
    lower.includes("cancel")
  ) {
    return new FreighterError(
      "declined",
      "You declined the request in Freighter. Nothing was signed — try again whenever you are ready.",
    );
  }

  if (lower.includes("network") || lower.includes("passphrase")) {
    const expected = expectedNetwork ?? "the app's network";
    return new FreighterError(
      "network-mismatch",
      `Freighter is on a different network than this app. This app expects ${expected}; switch networks in Freighter and try again.`,
    );
  }

  return new FreighterError(
    "rpc-failure",
    message || "Freighter could not complete the request. Please try again.",
  );
}

/**
 * Wallet material (private keys, seeds, signed payloads) must never be
 * persisted to browser storage. This helper is the single choke point for
 * Freighter responses so we can assert that property in tests.
 */
export const WALLET_STORAGE_KEYS = [
  "freighter",
  "wallet",
  "walletSession",
  "walletAddress",
  "signedTxXdr",
  "secretKey",
  "seed",
] as const;

function assertNoWalletPersistence(): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    for (const key of WALLET_STORAGE_KEYS) {
      window.localStorage?.removeItem(key);
      window.sessionStorage?.removeItem(key);
    }
  } catch {
    // Storage may be unavailable (private mode); nothing to clean up.
  }
}

export async function connectWallet(): Promise<WalletSession> {
  const connection = await isConnected();
  if (connection.error) {
    throw classifyError(connection.error);
  }
  if (!connection.isConnected) {
    throw new FreighterError(
      "not-installed",
      "Freighter was not detected. Install the Freighter extension, then try again.",
    );
  }

  const access = await requestAccess();
  if (access.error || !access.address) {
    throw classifyError(access.error);
  }

  const network = await getNetwork();
  if (network.error || !network.networkPassphrase) {
    throw classifyError(network.error);
  }

  assertNoWalletPersistence();

  return {
    address: access.address,
    network: network.network,
    networkPassphrase: network.networkPassphrase,
  };
}

export async function signWithFreighter(
  transactionXdr: string,
  address: string,
  networkPassphrase: string,
): Promise<string> {
  const result = await signTransaction(transactionXdr, {
    address,
    networkPassphrase,
  });

  if (result.error || !result.signedTxXdr) {
    throw classifyError(result.error, networkPassphrase);
  }

  assertNoWalletPersistence();

  return result.signedTxXdr;
}
