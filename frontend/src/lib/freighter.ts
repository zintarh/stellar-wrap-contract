import {
  getNetwork,
  isConnected,
  requestAccess,
  signTransaction,
} from "@stellar/freighter-api";
import type { WalletSession } from "./types";

function freighterError(
  error: { message?: string } | string | undefined,
  fallback: string,
): Error {
  if (typeof error === "string") {
    return new Error(error);
  }
  return new Error(error?.message ?? fallback);
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
    throw freighterError(connection.error, "Could not reach Freighter.");
  }
  if (!connection.isConnected) {
    throw new Error(
      "Freighter was not detected. Install or unlock the extension, then try again.",
    );
  }

  const access = await requestAccess();
  if (access.error || !access.address) {
    throw freighterError(access.error, "Wallet access was not approved.");
  }

  const network = await getNetwork();
  if (network.error || !network.networkPassphrase) {
    throw freighterError(network.error, "Could not read the Freighter network.");
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
    throw freighterError(result.error, "Freighter did not sign the transaction.");
  }

  assertNoWalletPersistence();

  return result.signedTxXdr;
}
