import {
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  rpc,
  TransactionBuilder,
  xdr,
} from '@stellar/stellar-sdk';
import * as dotenv from 'dotenv';
import * as path from 'path';
import { IndexerDB } from './db';
import { SorobanFetcher } from './fetcher';
import { withRetries } from './retry';

dotenv.config({ path: path.resolve(__dirname, '../.env') });

const LEDGERS_PER_DAY = 17_280;
const STROOPS_PER_XLM = 10_000_000n;
const MAX_TTL_BATCH_SIZE = 100;

export interface IndexedWrapKey {
  user: string;
  period: number;
}

export interface RenewalCandidate extends IndexedWrapKey {
  liveUntilLedgerSeq: number;
}

export interface RenewalSummary {
  checked: number;
  missing: number;
  expired: number;
  due: number;
  submitted: number;
  unconfirmed: number;
  renewed: number;
  estimatedFeeStroops: bigint;
  actualFeeStroops: bigint;
}

export function selectTtlRenewals(
  records: IndexedWrapKey[],
  liveUntilByRecord: ReadonlyMap<string, number>,
  latestLedger: number,
  thresholdDays: number,
): { candidates: RenewalCandidate[]; missing: number; expired: number } {
  if (!Number.isInteger(thresholdDays) || thresholdDays <= 0) {
    throw new Error('TTL renewal threshold must be a positive whole number of days');
  }

  const thresholdLedger = latestLedger + thresholdDays * LEDGERS_PER_DAY;
  const candidates: RenewalCandidate[] = [];
  let missing = 0;
  let expired = 0;

  for (const record of records) {
    const liveUntilLedgerSeq = liveUntilByRecord.get(`${record.user}/${record.period}`);
    if (liveUntilLedgerSeq === undefined) {
      missing += 1;
    } else if (liveUntilLedgerSeq <= latestLedger + 1) {
      expired += 1;
    } else if (liveUntilLedgerSeq <= thresholdLedger) {
      candidates.push({ ...record, liveUntilLedgerSeq });
    }
  }

  return { candidates, missing, expired };
}

export function groupTtlRenewalBatches(
  candidates: RenewalCandidate[],
): Array<{ user: string; periods: number[] }> {
  const byUser = new Map<string, number[]>();
  for (const candidate of candidates) {
    const periods = byUser.get(candidate.user) ?? [];
    periods.push(candidate.period);
    byUser.set(candidate.user, periods);
  }

  return [...byUser].flatMap(([user, periods]) =>
    chunks(periods, MAX_TTL_BATCH_SIZE).map((batch) => ({ user, periods: batch })),
  );
}

export function reserveTransactionFee(
  reservedFeeStroops: bigint,
  nextFeeStroops: bigint,
  maxSpendStroops: bigint,
): bigint {
  const total = reservedFeeStroops + nextFeeStroops;
  if (total > maxSpendStroops) {
    throw new Error(
      `TTL renewal would exceed spending cap: estimated ${formatXlm(total)} XLM, ` +
        `cap ${formatXlm(maxSpendStroops)} XLM`,
    );
  }
  return total;
}

export type RenewalRpcServer = Pick<
  rpc.Server,
  | 'getAccount'
  | 'getLedgerEntries'
  | 'getLatestLedger'
  | 'getTransaction'
  | 'sendTransaction'
  | 'simulateTransaction'
>;

export interface RenewalOptions {
  rpcUrl: string;
  contractId: string;
  sourceAccount: string;
  networkPassphrase: string;
  thresholdDays: number;
  maxSpendStroops: bigint;
  dryRun: boolean;
  signer?: Keypair;
  server?: RenewalRpcServer;
  log?: (message: string) => void;
}

function chunks<T>(values: T[], size: number): T[][] {
  const result: T[][] = [];
  for (let index = 0; index < values.length; index += size) {
    result.push(values.slice(index, index + size));
  }
  return result;
}

function formatXlm(stroops: bigint): string {
  const whole = stroops / STROOPS_PER_XLM;
  const fraction = (stroops % STROOPS_PER_XLM).toString().padStart(7, '0');
  return `${whole}.${fraction}`;
}

async function findRenewalCandidates(
  server: RenewalRpcServer,
  fetcher: SorobanFetcher,
  records: IndexedWrapKey[],
  latestLedger: number,
  thresholdDays: number,
): Promise<{ candidates: RenewalCandidate[]; missing: number; expired: number }> {
  const liveUntilByRecord = new Map<string, number>();

  for (const batch of chunks(records, MAX_TTL_BATCH_SIZE)) {
    const keyedRecords = batch.map((record) => ({
      record,
      key: fetcher.buildWrapKey(record.user, record.period),
    }));
    const requested = new Map(
      keyedRecords.map(({ record, key }) => [key.toXDR('base64'), record]),
    );
    const response = await withRetries(
      `getLedgerEntries(${batch.length} wrap keys)`,
      () => server.getLedgerEntries(...keyedRecords.map(({ key }) => key)),
    );

    for (const entry of response.entries) {
      const record = requested.get(entry.key.toXDR('base64'));
      if (!record) {
        throw new Error('Soroban RPC returned an unrequested TTL ledger key');
      }
      requested.delete(entry.key.toXDR('base64'));
      if (entry.liveUntilLedgerSeq === undefined) {
        throw new Error(
          `Soroban RPC omitted liveUntilLedgerSeq for ${record.user}/${record.period}`,
        );
      }
      liveUntilByRecord.set(`${record.user}/${record.period}`, entry.liveUntilLedgerSeq);
    }
  }

  return selectTtlRenewals(records, liveUntilByRecord, latestLedger, thresholdDays);
}

type RenewalReceipt = {
  status: rpc.Api.GetTransactionStatus.SUCCESS | rpc.Api.GetTransactionStatus.FAILED;
  feeStroops: bigint;
  ledger: number;
};

async function awaitTransaction(
  server: RenewalRpcServer,
  hash: string,
): Promise<RenewalReceipt> {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    const transaction = await server.getTransaction(hash);
    if (transaction.status === rpc.Api.GetTransactionStatus.SUCCESS) {
      return {
        status: transaction.status,
        feeStroops: BigInt(transaction.resultXdr.feeCharged().toString()),
        ledger: transaction.ledger,
      };
    }
    if (transaction.status === rpc.Api.GetTransactionStatus.FAILED) {
      return {
        status: transaction.status,
        feeStroops: BigInt(transaction.resultXdr.feeCharged().toString()),
        ledger: transaction.ledger,
      };
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  throw new Error(`Timed out waiting for TTL renewal transaction ${hash}`);
}

export async function renewIndexedWrapTtls(
  db: IndexerDB,
  options: RenewalOptions,
): Promise<RenewalSummary> {
  if (!Number.isInteger(options.thresholdDays) || options.thresholdDays <= 0) {
    throw new Error('TTL renewal threshold must be a positive whole number of days');
  }
  if (options.maxSpendStroops <= 0n) {
    throw new Error('TTL renewal spending cap must be greater than zero stroops');
  }
  if (!options.dryRun && !options.signer) {
    throw new Error('RENEWER_SECRET_KEY is required when dry-run mode is disabled');
  }
  if (options.signer && options.signer.publicKey() !== options.sourceAccount) {
    throw new Error('Renewal signer does not match RENEWER_SOURCE_ACCOUNT');
  }

  const log = options.log ?? console.log;
  const server = options.server ?? new rpc.Server(options.rpcUrl);
  const records = db.getWrapKeysForTtlRenewal(options.contractId);
  const summary: RenewalSummary = {
    checked: records.length,
    missing: 0,
    expired: 0,
    due: 0,
    submitted: 0,
    unconfirmed: 0,
    renewed: 0,
    estimatedFeeStroops: 0n,
    actualFeeStroops: 0n,
  };

  try {
    const latestLedger = await withRetries('getLatestLedger()', () =>
      server.getLatestLedger(),
    );
    const fetcher = new SorobanFetcher({
      rpcUrl: options.rpcUrl,
      contractId: options.contractId,
      eventPageSize: 1,
    });
    const found = await findRenewalCandidates(
      server,
      fetcher,
      records,
      latestLedger.sequence,
      options.thresholdDays,
    );
    summary.missing = found.missing;
    summary.expired = found.expired;
    summary.due = found.candidates.length;
    if (found.candidates.length === 0) {
      return summary;
    }

    const account = await withRetries(
      `getAccount(${options.sourceAccount})`,
      () => server.getAccount(options.sourceAccount),
    );
    const contract = new Contract(options.contractId);
    let reservedFeeStroops = 0n;

    for (const { user, periods } of groupTtlRenewalBatches(found.candidates)) {
      const transaction = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: options.networkPassphrase,
      })
        .addOperation(
          contract.call(
            'extend_ttl_batch',
            Address.fromString(user).toScVal(),
            xdr.ScVal.scvVec(
              periods.map((period) =>
                xdr.ScVal.scvU64(new xdr.Uint64(period)),
              ),
            ),
          ),
        )
        .setTimeout(60)
        .build();
      const simulation = await withRetries(
        `simulate TTL renewal for ${user}`,
        () => server.simulateTransaction(transaction),
      );
      if (rpc.Api.isSimulationError(simulation)) {
        throw new Error(`TTL renewal simulation failed for ${user}: ${simulation.error}`);
      }

      const prepared = rpc.assembleTransaction(transaction, simulation).build();
      const transactionFee = BigInt(prepared.fee);
      reservedFeeStroops = reserveTransactionFee(
        reservedFeeStroops,
        transactionFee,
        options.maxSpendStroops,
      );
      summary.estimatedFeeStroops = reservedFeeStroops;

      log(
        `${options.dryRun ? 'DRY RUN' : 'SUBMIT'} user=${user} records=${periods.length} ` +
          `fee_cap=${formatXlm(transactionFee)} XLM`,
      );
      if (options.dryRun) {
        continue;
      }

      if (!options.signer) {
        throw new Error('RENEWER_SECRET_KEY is required when dry-run mode is disabled');
      }
      prepared.sign(options.signer);
      const submitted = await server.sendTransaction(prepared);
      if (submitted.status === 'ERROR' || submitted.status === 'TRY_AGAIN_LATER') {
        throw new Error(
          `TTL renewal submission for ${user} was rejected (${submitted.status})`,
        );
      }

      summary.submitted += 1;
      let receipt: RenewalReceipt;
      try {
        receipt = await awaitTransaction(server, submitted.hash);
      } catch (error) {
        summary.unconfirmed += 1;
        log(
          `Transaction ${submitted.hash} has no confirmed outcome; ` +
            `up to ${formatXlm(transactionFee)} XLM may have been charged`,
        );
        throw error;
      }
      summary.actualFeeStroops += receipt.feeStroops;
      log(
        `Transaction ${submitted.hash} status=${receipt.status}; ` +
          `spent=${formatXlm(receipt.feeStroops)} XLM`,
      );
      if (receipt.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
        throw new Error(
          `TTL renewal transaction ${submitted.hash} failed on ledger ${receipt.ledger}`,
        );
      }
      summary.renewed += periods.length;
      log(`records_renewed=${summary.renewed}`);
    }

    return summary;
  } finally {
    log(
      `TTL renewal metrics: checked=${summary.checked} due=${summary.due} ` +
        `renewed=${summary.renewed} missing=${summary.missing} expired=${summary.expired} ` +
        `submitted=${summary.submitted} unconfirmed=${summary.unconfirmed} ` +
        `estimated_fee=${formatXlm(summary.estimatedFeeStroops)} XLM ` +
        `actual_spent_confirmed=${formatXlm(summary.actualFeeStroops)} XLM ` +
        `cap=${formatXlm(options.maxSpendStroops)} XLM`,
    );
  }
}

function readPositiveInteger(name: string, fallback?: number): number {
  const raw = process.env[name];
  const value = raw === undefined ? fallback : Number(raw);
  if (value === undefined || !Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive whole number`);
  }
  return value;
}

async function main(): Promise<void> {
  const contractId = process.env.CONTRACT_ID;
  const rpcUrl = process.env.RPC_URL || 'https://soroban-testnet.stellar.org';
  const databasePath = process.env.DB_PATH || path.resolve(__dirname, '../indexer.db');
  const sourceAccount = process.env.RENEWER_SOURCE_ACCOUNT;
  const networkPassphrase = process.env.STELLAR_NETWORK_PASSPHRASE;
  if (!contractId || !sourceAccount || !networkPassphrase) {
    throw new Error(
      'CONTRACT_ID, RENEWER_SOURCE_ACCOUNT, and STELLAR_NETWORK_PASSPHRASE are required',
    );
  }

  const dryRun = !process.argv.includes('--execute');
  const maxSpendStroops = BigInt(
    process.env.RENEWER_MAX_SPEND_STROOPS || '0',
  );
  const secret = process.env.RENEWER_SECRET_KEY;
  if (!dryRun && !secret) {
    throw new Error('RENEWER_SECRET_KEY is required with --execute');
  }
  const signer = secret ? Keypair.fromSecret(secret) : undefined;
  const db = await IndexerDB.create(databasePath);
  try {
    await renewIndexedWrapTtls(db, {
      rpcUrl,
      contractId,
      sourceAccount,
      networkPassphrase,
      thresholdDays: readPositiveInteger('RENEWER_THRESHOLD_DAYS', 30),
      maxSpendStroops,
      dryRun,
      signer,
    });
  } finally {
    db.close();
  }
}

if (require.main === module) {
  main().catch((error: unknown) => {
    console.error('TTL renewal failed:', error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
