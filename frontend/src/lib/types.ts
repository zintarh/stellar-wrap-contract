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
 * Distinct outcomes of a Freighter wallet interaction. These are kept separate
 * because each one has a different correct response: a missing extension or a
 * locked wallet needs setup, a declined signature is a user choice to
 * acknowledge (never an application error), a network mismatch needs the
 * wallet switched, and only a genuine RPC failure is an actual error.
 */
export type WalletErrorKind =
  | "not-installed"
  | "locked"
  | "declined"
  | "network-mismatch"
  | "rpc-failure";

export type WalletError = {
  kind: WalletErrorKind;
  /** Human-readable message telling the user what to do next. */
  message: string;
  /** True only for genuine failures; false for user choices like a decline. */
  isError: boolean;
  /** Whether the operation may be retried automatically. */
  retryable: boolean;
  /** Network the app expects, present on a network mismatch. */
  expectedNetwork?: string;
  /** Network the wallet is currently on, present on a network mismatch. */
  actualNetwork?: string;
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
