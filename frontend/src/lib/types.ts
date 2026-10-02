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
