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
   * Lifecycle flags for the record. Optional so callers that only care
   * about the happy path can ignore them; a record with none of them set
   * is treated as active.
   */
  revoked?: boolean;
  burned?: boolean;
  expired?: boolean;
  optedOut?: boolean;
};

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
