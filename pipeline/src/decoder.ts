import {
  xdr,
  scValToNative,
  nativeToScVal,
  Address,
} from '@stellar/stellar-sdk';
import type {
  DecodedKey,
  DecodedStorageValue,
  EventTopic,
  StorageEntry,
  WrapRecord,
  WrapState,
  FeeParams,
  WrapLifecycleFSM,
} from './types';
import { DataKeyVariant } from './types';

// ─── ScVal helpers ──────────────────────────────────────────────────────

function scvToAddress(val: xdr.ScVal): string {
  return Address.fromScVal(val).toString();
}

function scvToBytesN(val: xdr.ScVal, length: number): string {
  const buf = val.bytes() as Buffer;
  if (buf.length !== length) {
    throw new Error(`Expected ${length} bytes, got ${buf.length}`);
  }
  return buf.toString('hex');
}

function scvToSymbol(val: xdr.ScVal): string {
  return val.sym().toString();
}

function scvToU64(val: xdr.ScVal): number {
  const u64 = val.u64();
  return Number(u64.toString());
}

function scvToI128(val: xdr.ScVal): bigint {
  const i128 = val.i128();
  return BigInt(i128.toString());
}

function scvToBool(val: xdr.ScVal): boolean {
  return val.b() === true;
}

function scvToString(val: xdr.ScVal): string {
  if (val.str()) {
    return val.str().toString();
  }
  return scValToNative(val);
}

// ─── DataKey decoder ────────────────────────────────────────────────────

export function decodeDataKey(keyScVal: xdr.ScVal): DecodedKey {
  const keyVec = keyScVal.vec();
  if (keyVec === null || keyVec === undefined) {
    // Instance storage: key is wrapped as VecM of length 1 (the variant symbol)
    if (keyScVal.sym()) {
      return decodeSingletonKey(keyScVal.sym().toString());
    }
    throw new Error(`Unsupported key format: ${keyScVal.toXDR('base64')}`);
  }

  if (keyVec.length === 0) {
    throw new Error('Empty vec key');
  }

  const variant = keyVec[0];
  if (!variant.sym()) {
    throw new Error(`First element of key vec must be a symbol, got: ${variant.toXDR('base64')}`);
  }

  const variantName = variant.sym().toString();
  switch (variantName) {
    // Instance singleton keys (represented as vec for instance storage in snapshots)
    case 'Admin':
      return { variant: DataKeyVariant.Admin };
    case 'AdminPubKey':
      return { variant: DataKeyVariant.AdminPubKey };
    case 'PendingAdmin':
      return { variant: DataKeyVariant.PendingAdmin };
    case 'MigrationVersion':
      return { variant: DataKeyVariant.MigrationVersion };
    case 'TotalWrapCount':
      return { variant: DataKeyVariant.TotalWrapCount };
    case 'TotalRevoked':
      return { variant: DataKeyVariant.TotalRevoked };
    case 'Name':
      return { variant: DataKeyVariant.Name };
    case 'Symbol':
      return { variant: DataKeyVariant.Symbol };
    case 'Paused':
      return { variant: DataKeyVariant.Paused };
    case 'StorageBytes':
      return { variant: DataKeyVariant.StorageBytes };
    case 'FeeParams':
      return { variant: DataKeyVariant.FeeParams };
    case 'SlashThreshold':
      return { variant: DataKeyVariant.SlashThreshold };

    // Keys with Address param
    case 'WrapCount': {
      if (keyVec.length < 2) throw new Error('WrapCount needs an address');
      return { variant: DataKeyVariant.WrapCount, user: scvToAddress(keyVec[1]) };
    }
    case 'LatestPeriod': {
      if (keyVec.length < 2) throw new Error('LatestPeriod needs an address');
      return { variant: DataKeyVariant.LatestPeriod, user: scvToAddress(keyVec[1]) };
    }
    case 'UserPeriods': {
      if (keyVec.length < 2) throw new Error('UserPeriods needs an address');
      return { variant: DataKeyVariant.UserPeriods, user: scvToAddress(keyVec[1]) };
    }
    case 'AliasHash': {
      if (keyVec.length < 2) throw new Error('AliasHash needs an address');
      return { variant: DataKeyVariant.AliasHash, user: scvToAddress(keyVec[1]) };
    }
    case 'SlashCount': {
      if (keyVec.length < 2) throw new Error('SlashCount needs an address');
      return { variant: DataKeyVariant.SlashCount, user: scvToAddress(keyVec[1]) };
    }
    case 'Slashed': {
      if (keyVec.length < 2) throw new Error('Slashed needs an address');
      return { variant: DataKeyVariant.Slashed, user: scvToAddress(keyVec[1]) };
    }

    // Wrap(Address, u64)
    case 'Wrap': {
      if (keyVec.length < 3) throw new Error('Wrap needs address and period');
      return {
        variant: DataKeyVariant.Wrap,
        user: scvToAddress(keyVec[1]),
        period: scvToU64(keyVec[2]),
      };
    }

    default:
      throw new Error(`Unknown DataKey variant: ${variantName}`);
  }
}

function decodeSingletonKey(name: string): DecodedKey {
  switch (name) {
    case 'Admin': return { variant: DataKeyVariant.Admin };
    case 'AdminPubKey': return { variant: DataKeyVariant.AdminPubKey };
    case 'PendingAdmin': return { variant: DataKeyVariant.PendingAdmin };
    case 'MigrationVersion': return { variant: DataKeyVariant.MigrationVersion };
    case 'TotalWrapCount': return { variant: DataKeyVariant.TotalWrapCount };
    case 'TotalRevoked': return { variant: DataKeyVariant.TotalRevoked };
    case 'Name': return { variant: DataKeyVariant.Name };
    case 'Symbol': return { variant: DataKeyVariant.Symbol };
    case 'Paused': return { variant: DataKeyVariant.Paused };
    case 'StorageBytes': return { variant: DataKeyVariant.StorageBytes };
    case 'FeeParams': return { variant: DataKeyVariant.FeeParams };
    case 'SlashThreshold': return { variant: DataKeyVariant.SlashThreshold };
    default: throw new Error(`Unknown singleton key: ${name}`);
  }
}

// ─── Value decoder ──────────────────────────────────────────────────────

function decodeWrapRecord(valMap: xdr.ScMapEntry[]): WrapRecord {
  const record: Record<string, unknown> = {};
  for (const entry of valMap) {
    const key = entry.key().sym().toString();
    const raw = entry.val();

    switch (key) {
      case 'created_at':
        record.created_at = scvToU64(raw);
        break;
      case 'data_hash':
        record.data_hash = scvToBytesN(raw, 32);
        break;
      case 'archetype':
        record.archetype = scvToSymbol(raw);
        break;
      case 'period':
        record.period = scvToU64(raw);
        break;
      case 'lifecycle': {
        const lifecycleMap = raw.map() ?? [];
        record.lifecycle = decodeFSM(lifecycleMap);
        break;
      }
    }
  }

  return {
    created_at: record.created_at as number,
    data_hash: record.data_hash as string,
    archetype: record.archetype as string,
    period: record.period as number,
    lifecycle: record.lifecycle as WrapLifecycleFSM ?? { state: 3, updated_at: record.created_at as number },
  };
}

function decodeFSM(map: xdr.ScMapEntry[]): WrapLifecycleFSM {
  let state: WrapState = 3; // default Active
  let updated_at = 0;

  for (const entry of map) {
    const key = entry.key().sym().toString();
    const raw = entry.val();
    if (key === 'state') {
      state = scvToU64(raw) as WrapState;
    } else if (key === 'updated_at') {
      updated_at = scvToU64(raw);
    }
  }

  return { state, updated_at };
}

function decodeFeeParams(valMap: xdr.ScMapEntry[]): FeeParams {
  let baseFee = 0n;
  let perKibFee = 0n;
  let scaleStepKib = 1;
  let maxFee = BigInt(Number.MAX_SAFE_INTEGER);

  for (const entry of valMap) {
    const key = entry.key().sym().toString();
    const raw = entry.val();
    switch (key) {
      case 'base_fee':
        baseFee = scvToI128(raw);
        break;
      case 'per_kib_fee':
        perKibFee = scvToI128(raw);
        break;
      case 'scale_step_kib':
        scaleStepKib = scvToU64(raw);
        break;
      case 'max_fee':
        maxFee = scvToI128(raw);
        break;
    }
  }

  return {
    base_fee: baseFee,
    per_kib_fee: perKibFee,
    scale_step_kib: scaleStepKib,
    max_fee: maxFee,
  };
}

function decodeVecU64(val: xdr.ScVal): number[] {
  const v = val.vec();
  if (!v) return [];
  const result: number[] = [];
  for (const element of v) {
    result.push(scvToU64(element));
  }
  return result;
}

export function decodeStorageValue(
  key: DecodedKey,
  valueScVal: xdr.ScVal,
): DecodedStorageValue {
  switch (key.variant) {
    case DataKeyVariant.Admin:
    case DataKeyVariant.PendingAdmin:
      return { type: 'address', value: Address.fromScVal(valueScVal).toString() };

    case DataKeyVariant.AdminPubKey:
    case DataKeyVariant.AliasHash:
      return { type: 'bytes32', value: scvToBytesN(valueScVal, 32) };

    case DataKeyVariant.MigrationVersion:
    case DataKeyVariant.TotalWrapCount:
    case DataKeyVariant.WrapCount:
    case DataKeyVariant.SlashCount:
    case DataKeyVariant.SlashThreshold:
      return { type: 'u32', value: Number(scValToNative(valueScVal)) };

    case DataKeyVariant.LatestPeriod:
    case DataKeyVariant.TotalRevoked:
    case DataKeyVariant.StorageBytes:
      return { type: 'u64', value: scvToU64(valueScVal) };

    case DataKeyVariant.Paused:
    case DataKeyVariant.Slashed:
      return { type: 'bool', value: scvToBool(valueScVal) };

    case DataKeyVariant.Name:
    case DataKeyVariant.Symbol:
      return { type: 'string', value: scvToString(valueScVal) };

    case DataKeyVariant.Wrap: {
      const valMap = valueScVal.map() ?? [];
      return { type: 'wrap_record', value: decodeWrapRecord(valMap) };
    }

    case DataKeyVariant.FeeParams: {
      const valMap = valueScVal.map() ?? [];
      return { type: 'fee_params', value: decodeFeeParams(valMap) };
    }

    case DataKeyVariant.UserPeriods:
      return { type: 'u64_vec', value: decodeVecU64(valueScVal) };

    default:
      throw new Error(`decodeStorageValue not implemented for ${(key as DecodedKey).variant}`);
  }
}

// ─── Event decoder ──────────────────────────────────────────────────────

export function decodeEventTopic(val: xdr.ScVal): EventTopic {
  switch (val.switch().name) {
    case 'scvSymbol':
      return { type: 'symbol', value: val.sym().toString() };
    case 'scvAddress':
      return { type: 'address', value: Address.fromScVal(val).toString() };
    case 'scvU64':
      return { type: 'u64', value: scvToU64(val) };
    case 'scvU32':
      return { type: 'u32', value: val.u32() };
    case 'scvBool':
      return { type: 'bool', value: val.b() };
    case 'scvBytes':
      return { type: 'bytes', value: Buffer.from(val.bytes()).toString('hex') };
    default:
      return { type: 'symbol', value: scValToNative(val)?.toString() ?? 'unknown' };
  }
}

export function decodeEventData(val: xdr.ScVal): DecodedStorageValue {
  const switchName = val.switch().name;
  switch (switchName) {
    case 'scvSymbol':
      return { type: 'string', value: val.sym().toString() };
    case 'scvAddress':
      return { type: 'address', value: Address.fromScVal(val).toString() };
    case 'scvU64':
      return { type: 'u64', value: scvToU64(val) };
    case 'scvU32':
      return { type: 'u32', value: val.u32() };
    case 'scvBool':
      return { type: 'bool', value: val.b() };
    case 'scvBytes':
      return { type: 'bytes', value: Buffer.from(val.bytes()).toString('hex') };
    case 'scvMap':
      return { type: 'string', value: JSON.stringify(scValToNative(val)) };
    case 'scvVec':
      return { type: 'string', value: JSON.stringify(scValToNative(val)) };
    default:
      return { type: 'string', value: JSON.stringify(scValToNative(val)) };
  }
}

// ─── Storage entry decoder ──────────────────────────────────────────────

export function decodeLedgerEntry(
  ledgerKey: xdr.LedgerKey,
  ledgerEntry: xdr.LedgerEntry,
): StorageEntry | null {
  const ledgerData = ledgerEntry.data();

  if (ledgerData.switch().name !== 'contractData') {
    return null;
  }

  const contractData = ledgerData.contractData();
  const keyScVal = contractData.key();
  const valScVal = contractData.val();
  const durability = contractData.durability().name.toLowerCase() as 'persistent' | 'temporary' | 'instance';

  const decodedKey = decodeDataKey(keyScVal);
  const decodedValue = decodeStorageValue(decodedKey, valScVal);

  return {
    key: decodedKey,
    value: decodedValue,
    ledger: ledgerEntry.lastModifiedLedgerSeq(),
    durability,
  };
}
