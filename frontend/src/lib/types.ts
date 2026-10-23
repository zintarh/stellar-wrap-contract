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
   * only use the legacy query shape remain compatible.
   */
  revoked?: boolean;
  burned?: boolean;
  expired?: boolean;
  optedOut?: boolean;
};

export type WrapView = {
  record: WrapRecord;
  isExpired: boolean;
  liveUntilLedger: bigint;
};

export type Dashboard = {
  health: ContractHealth;
  balance: bigint;
  latestWrap: WrapRecord | null;
  records: WrapRecord[];
};

export type MintInput = {
  archetype: string;
  dataHash: Uint8Array;
  period: bigint;
  signature: Uint8Array;
};
