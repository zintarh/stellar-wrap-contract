import { Address } from '@stellar/stellar-sdk';

// ─── On-chain enum mirrors ─────────────────────────────────────────────

export enum WrapState {
  Draft = 1,
  Pending = 2,
  Active = 3,
  Archived = 4,
  Cancelled = 5,
}

export enum ContractErrorCode {
  AlreadyInitialized = 1,
  NotInitialized = 2,
  Unauthorized = 3,
  WrapAlreadyExists = 4,
  InvalidSignature = 5,
  InvalidPeriod = 6,
  MigrationAlreadyApplied = 7,
  InvalidStateTransition = 8,
  WrapNotFound = 9,
  NoAdminTransferProposal = 10,
  AdminTransferProposalExists = 11,
  Paused = 12,
  ArithmeticOverflow = 13,
  InvalidFeeParams = 14,
  Slashed = 15,
}

// ─── DataKey variant discriminator ──────────────────────────────────────

export enum DataKeyVariant {
  Admin = 'Admin',
  AdminPubKey = 'AdminPubKey',
  PendingAdmin = 'PendingAdmin',
  Wrap = 'Wrap',
  WrapCount = 'WrapCount',
  LatestPeriod = 'LatestPeriod',
  MigrationVersion = 'MigrationVersion',
  UserPeriods = 'UserPeriods',
  TotalWrapCount = 'TotalWrapCount',
  TotalRevoked = 'TotalRevoked',
  AliasHash = 'AliasHash',
  Name = 'Name',
  Symbol = 'Symbol',
  Paused = 'Paused',
  StorageBytes = 'StorageBytes',
  FeeParams = 'FeeParams',
  SlashCount = 'SlashCount',
  Slashed = 'Slashed',
  SlashThreshold = 'SlashThreshold',
}

// ─── Decoded storage key ────────────────────────────────────────────────

export type DecodedKey =
  | { variant: DataKeyVariant.Admin }
  | { variant: DataKeyVariant.AdminPubKey }
  | { variant: DataKeyVariant.PendingAdmin }
  | { variant: DataKeyVariant.Wrap; user: string; period: number }
  | { variant: DataKeyVariant.WrapCount; user: string }
  | { variant: DataKeyVariant.LatestPeriod; user: string }
  | { variant: DataKeyVariant.MigrationVersion }
  | { variant: DataKeyVariant.UserPeriods; user: string }
  | { variant: DataKeyVariant.TotalWrapCount }
  | { variant: DataKeyVariant.TotalRevoked }
  | { variant: DataKeyVariant.AliasHash; user: string }
  | { variant: DataKeyVariant.Name }
  | { variant: DataKeyVariant.Symbol }
  | { variant: DataKeyVariant.Paused }
  | { variant: DataKeyVariant.StorageBytes }
  | { variant: DataKeyVariant.FeeParams }
  | { variant: DataKeyVariant.SlashCount; user: string }
  | { variant: DataKeyVariant.Slashed; user: string }
  | { variant: DataKeyVariant.SlashThreshold };

// ─── Value types ────────────────────────────────────────────────────────

export interface WrapLifecycleFSM {
  state: WrapState;
  updated_at: number;
}

export interface WrapRecord {
  created_at: number;
  data_hash: string; // hex-encoded 32 bytes
  archetype: string;
  period: number;
  lifecycle: WrapLifecycleFSM;
}

export interface FeeParams {
  base_fee: bigint;
  per_kib_fee: bigint;
  scale_step_kib: number;
  max_fee: bigint;
}

export interface ContractHealth {
  initialized: boolean;
  has_admin: boolean;
  has_signing_key: boolean;
}

// ─── Storage entry (decoded) ────────────────────────────────────────────

export type DecodedStorageValue =
  | { type: 'address'; value: string }
  | { type: 'bytes32'; value: string }
  | { type: 'u64'; value: number }
  | { type: 'u32'; value: number }
  | { type: 'bool'; value: boolean }
  | { type: 'string'; value: string }
  | { type: 'wrap_record'; value: WrapRecord }
  | { type: 'wrap_state'; value: WrapState }
  | { type: 'fee_params'; value: FeeParams }
  | { type: 'bytes'; value: string }
  | { type: 'u64_vec'; value: number[] }
  | { type: 'i128'; value: bigint };

export interface StorageEntry {
  key: DecodedKey;
  value: DecodedStorageValue;
  ledger: number;
  durability: 'persistent' | 'temporary' | 'instance';
}

// ─── Event types ────────────────────────────────────────────────────────

export type EventTopic =
  | { type: 'symbol'; value: string }
  | { type: 'address'; value: string }
  | { type: 'u64'; value: number }
  | { type: 'u32'; value: number }
  | { type: 'bool'; value: boolean }
  | { type: 'bytes'; value: string };

export interface ContractEvent {
  id: string;
  contract_id: string;
  ledger: number;
  ledger_close_at: string | null;
  tx_hash: string;
  topics: EventTopic[];
  data: DecodedStorageValue;
  failed_call: boolean;
}

export type EventType =
  | 'mint'
  | 'revoke'
  | 'transition'
  | 'init'
  | 'pause'
  | 'upgrade'
  | 'admin_update'
  | 'slash_report'
  | 'slash_clear'
  | 'slash_threshold'
  | 'unknown';

export interface TypedEvent {
  raw: ContractEvent;
  event_type: EventType;
  parsed: Record<string, unknown>;
}

// ─── Database row types ─────────────────────────────────────────────────

export interface WrapRow {
  contract_id: string;
  user: string;
  period: number;
  timestamp: number;
  data_hash: string;
  archetype: string;
  fsm_state: number;
  fsm_updated_at: number;
  ledger_seq: number;
  tx_hash: string;
  created_at: string;
  updated_at: string;
}

export interface EventRow {
  id: string;
  contract_id: string;
  event_type: string;
  ledger_seq: number;
  tx_hash: string;
  topics_json: string;
  data_json: string;
  failed_call: boolean;
  created_at: string;
}

export interface UserStateRow {
  contract_id: string;
  user: string;
  wrap_count: number;
  latest_period: number | null;
  alias_hash: string | null;
  slash_count: number;
  is_slashed: boolean;
  periods_json: string;
  ledger_seq: number;
  updated_at: string;
}

export interface ContractStateRow {
  contract_id: string;
  admin: string | null;
  admin_pubkey: string | null;
  pending_admin: string | null;
  migration_version: number;
  is_paused: boolean;
  total_wrap_count: number;
  total_revoked: number;
  storage_bytes: number;
  slash_threshold: number;
  ledger_seq: number;
  updated_at: string;
}

export interface LedgerCursorRow {
  id: string;
  contract_id: string;
  last_processed_ledger: number;
  last_event_ledger: number;
  updated_at: string;
}

// ─── Derived State (in-memory index) ─────────────────────────────────────

export interface DerivedState {
  contract_id: string;
  ledger_seq: number;
  wraps: Map<string, Map<number, WrapRecord>>;
  userCounts: Map<string, number>;
  userLatestPeriods: Map<string, number>;
  userPeriods: Map<string, number[]>;
  userAliasHashes: Map<string, string>;
  userSlashCounts: Map<string, number>;
  userSlashed: Map<string, boolean>;
  admin: string | null;
  adminPubKey: string | null;
  pendingAdmin: string | null;
  migrationVersion: number;
  paused: boolean;
  totalWrapCount: number;
  totalRevoked: number;
  storageBytes: number;
  slashThreshold: number;
  name: string | null;
  symbol: string | null;
}

// ─── Configuration ──────────────────────────────────────────────────────

export interface IndexerConfig {
  rpc_url: string;
  contract_id: string;
  db_path: string;
  poll_interval_ms: number;
  event_page_size: number;
  start_ledger: number;
  backfill: boolean;
  reconcile_only: boolean;
  alert_webhook_url: string;
  alert_min_severity: 'info' | 'warning' | 'critical';
  expected_admin: string;
}
