export type NetworkConfig = {
  contractId: string;
  rpcUrl: string;
  networkPassphrase: string;
};

export type WalletSession = {
  address: string;
  network: string;
  networkPassphrase: string;
};

export type ContractHealth = {
  initialized: boolean;
  hasAdmin: boolean;
  hasSigningKey: boolean;
};

/**
 * Lifecycle state of a wrap record. The contract supports revoke, burn,
 * opt-out and expiration, so a record is more than just "exists".
 */
export type WrapRecordState =
  | "active"
  | "revoked"
  | "burned"
  | "expired"
  | "opted-out";

export type WrapRecord = {
  createdAt: bigint;
  dataHash: string;
  archetype: string;
  /** Raw period as stored on-chain, in `YYYYMM` form. */
  period: bigint;
  /**
   * Current lifecycle state of the record. Optional so existing callers that
   * only know a record exists keep working; treat a missing value as "active".
   */
  state?: WrapRecordState;
  revoked?: boolean;
  burned?: boolean;
  optedOut?: boolean;
  expired?: boolean;
  /**
   * Records are soulbound: they cannot be transferred between accounts.
   * Kept on the record so the UI can surface this without extra lookups.
   */
  soulbound?: boolean;
};

/**
 * Human-readable label for a record state, suitable for badges and lists.
 */
export const WRAP_RECORD_STATE_LABELS: Record<WrapRecordState, string> = {
  active: "Active",
  revoked: "Revoked",
  burned: "Burned",
  expired: "Expired",
  "opted-out": "Opted out",
};

/**
 * Format a raw `YYYYMM` period into a readable form (e.g. "March 2024")
 * while keeping the raw value available to callers.
 */
export function formatWrapPeriod(period: bigint): string {
  const raw = period.toString().padStart(6, "0");
  const year = Number(raw.slice(0, 4));
  const month = Number(raw.slice(4, 6));
  if (!Number.isFinite(year) || month < 1 || month > 12) {
    return raw;
  }
  const date = new Date(Date.UTC(year, month - 1, 1));
  return date.toLocaleDateString("en-US", {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
}

export type Dashboard = {
  balance: bigint;
  health: ContractHealth;
  latestWrap: WrapRecord | null;
  records: WrapRecord[];
};

export type MintInput = {
  period: bigint;
  archetype: string;
  dataHash: Uint8Array;
  signature: Uint8Array;
};

/**
 * Keys that must never be written to browser storage. Wallet material
 * (private keys, seeds, signed payloads) is held only in memory for the
 * lifetime of a request and is never persisted.
 */
export const FORBIDDEN_STORAGE_KEYS = [
  "privateKey",
  "private_key",
  "secretKey",
  "secret_key",
  "seed",
  "mnemonic",
  "signature",
  "signedPayload",
  "signed_payload",
  "signedXdr",
  "signed_xdr",
] as const;

/**
 * Returns true when a storage key looks like wallet material that must not
 * be persisted to localStorage, sessionStorage, or a cookie.
 */
export function isForbiddenStorageKey(key: string): boolean {
  const normalized = key.toLowerCase();
  return FORBIDDEN_STORAGE_KEYS.some((forbidden) =>
    normalized.includes(forbidden.toLowerCase()),
  );
}

/**
 * Assert that a value destined for browser storage contains no wallet
 * material. Throws when a forbidden key is present so callers fail closed
 * instead of silently persisting sensitive data.
 */
export function assertNoWalletMaterial(
  value: Record<string, unknown>,
): void {
  for (const key of Object.keys(value)) {
    if (isForbiddenStorageKey(key)) {
      throw new Error(
        `Refusing to persist wallet material under key "${key}"`,
      );
    }
  }
}

/**
 * Narrow an untrusted RPC/contract response to a plain object before it is
 * read or rendered. Contract responses are treated as untrusted input.
 */
export function isPlainObject(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype
  );
}

/**
 * Validate the shape of an untrusted contract health response before it is
 * rendered. Returns null when the response does not match the expected shape.
 */
export function parseContractHealth(value: unknown): ContractHealth | null {
  if (!isPlainObject(value)) {
    return null;
  }
  const { initialized, hasAdmin, hasSigningKey } = value;
  if (
    typeof initialized !== "boolean" ||
    typeof hasAdmin !== "boolean" ||
    typeof hasSigningKey !== "boolean"
  ) {
    return null;
  }
  return { initialized, hasAdmin, hasSigningKey };
}
