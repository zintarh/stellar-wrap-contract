import { rpc, xdr, Address } from '@stellar/stellar-sdk';
import type { ContractEvent, StorageEntry } from './types';
import { decodeEventTopic, decodeEventData, decodeLedgerEntry } from './decoder';

export interface FetcherOptions {
  rpcUrl: string;
  contractId: string;
  eventPageSize: number;
}

export class SorobanFetcher {
  private server: rpc.Server;
  private contractId: string;
  private eventPageSize: number;

  constructor(opts: FetcherOptions) {
    this.server = new rpc.Server(opts.rpcUrl);
    this.contractId = opts.contractId;
    this.eventPageSize = opts.eventPageSize;
  }

  /**
   * Fetch contract events from Soroban-RPC.
   * Returns events and the latest ledger sequence seen.
   */
  async fetchEvents(
    startLedger: number,
    topicsFilter?: string[],
  ): Promise<{ events: ContractEvent[]; latestLedger: number }> {
    const filters: rpc.Api.EventFilter[] = [
      {
        type: 'contract',
        contractIds: [this.contractId],
        topics: topicsFilter
          ? topicsFilter.map((t) => [
              xdr.ScVal.scvSymbol(t).toXDR('base64'),
            ])
          : undefined,
      },
    ];

    const events: ContractEvent[] = [];
    let cursor: string | undefined;
    let latestLedger = startLedger;
    let pageSize: number;
    do {
      const response = await this.server.getEvents({
        startLedger,
        cursor,
        filters,
        limit: this.eventPageSize,
      });
      const page = response.events || [];
      events.push(...page.map((event) => this.parseRawEvent(event)));
      const previousCursor = cursor;
      cursor = response.cursor;
      latestLedger = response.latestLedger;
      pageSize = page.length;
      if (pageSize === this.eventPageSize && cursor === previousCursor) {
        throw new Error('Soroban RPC event cursor did not advance.');
      }
    } while (pageSize === this.eventPageSize);

    return {
      events,
      latestLedger,
    };
  }

  private parseRawEvent(raw: rpc.Api.EventResponse): ContractEvent {
    const topics: ContractEvent['topics'] = raw.topic.map((t) =>
      t instanceof xdr.ScVal ? decodeEventTopic(t) : { type: 'symbol' as const, value: String(t) },
    );

    const data: ContractEvent['data'] = decodeEventData(raw.value as xdr.ScVal);

    return {
      id: raw.id,
      contract_id: String(raw.contractId),
      ledger: raw.ledger,
      ledger_close_at: null,
      tx_hash: raw.txHash,
      topics,
      data,
      failed_call: false,
    };
  }

  /**
   * Fetch current storage entries for the contract from Soroban-RPC.
   * This returns the latest state, not historical snapshots.
   */
  async fetchStorageEntries(): Promise<StorageEntry[]> {
    const response = await this.server.getLedgerEntries();
    const entries: StorageEntry[] = [];

    for (const rawEntry of response.entries || []) {
      try {
        // We need to iterate over all possible ledger keys.
        // The response returns all entries - we filter to our contract.
        const ledgerEntry = rawEntry as unknown as {
          key: xdr.LedgerKey;
          entry: xdr.LedgerEntry;
        };

        const decoded = decodeLedgerEntry(ledgerEntry.key, ledgerEntry.entry);
        if (decoded) {
          entries.push(decoded);
        }
      } catch (e) {
        // Skip entries we can't decode
        continue;
      }
    }

    return entries;
  }

  /**
   * Fetch specific storage entries by key.
   * Uses `getLedgerEntries` with specific ledger keys for targeted fetching.
   */
  async fetchStorageByKeys(ledgerKeys: xdr.LedgerKey[]): Promise<StorageEntry[]> {
    if (ledgerKeys.length === 0) return [];

    const response = await this.server.getLedgerEntries(...ledgerKeys);
    const entries: StorageEntry[] = [];

    for (const rawEntry of response.entries || []) {
      try {
        const ledgerEntry = rawEntry as unknown as {
          key: xdr.LedgerKey;
          entry: xdr.LedgerEntry;
        };
        const decoded = decodeLedgerEntry(ledgerEntry.key, ledgerEntry.entry);
        if (decoded) {
          entries.push(decoded);
        }
      } catch (e) {
        continue;
      }
    }

    return entries;
  }

  /**
   * Build a LedgerKey for DataKey::Wrap(user, period) for targeted fetching.
   */
  buildWrapKey(user: string, period: number): xdr.LedgerKey {
    const contractId = Address.fromString(this.contractId).toScAddress();
    const userAddr = Address.fromString(user).toScVal();
    const periodVal = xdr.ScVal.scvU64(new xdr.Uint64(period));

    const key = xdr.ScVal.scvVec([
      xdr.ScVal.scvSymbol('Wrap'),
      userAddr,
      periodVal,
    ]);

    return xdr.LedgerKey.contractData(
      new xdr.LedgerKeyContractData({
        durability: xdr.ContractDataDurability.persistent(),
        contract: contractId,
        key,
      }),
    );
  }

  /**
   * Fetch the latest ledger sequence from the network.
   */
  async getLatestLedger(): Promise<number> {
    const info = await this.server.getLatestLedger();
    return info.sequence;
  }

  /**
   * Build a LedgerKey for a general DataKey.
   */
  buildDataKey(variant: string, args: (string | number)[] = []): xdr.LedgerKey {
    const contractId = Address.fromString(this.contractId).toScAddress();

    const vec: xdr.ScVal[] = [xdr.ScVal.scvSymbol(variant)];
    for (const arg of args) {
      if (typeof arg === 'string') {
        vec.push(Address.fromString(arg).toScVal());
      } else {
        vec.push(xdr.ScVal.scvU64(new xdr.Uint64(arg)));
      }
    }

    const key = xdr.ScVal.scvVec(vec);

    const isInstance = ['Admin', 'AdminPubKey', 'PendingAdmin', 'MigrationVersion',
      'Paused', 'Name', 'Symbol', 'StorageBytes', 'FeeParams', 'SlashThreshold',
      'TotalRevoked'].includes(variant);

    return xdr.LedgerKey.contractData(
      new xdr.LedgerKeyContractData({
        durability: isInstance ? xdr.ContractDataDurability.persistent() : xdr.ContractDataDurability.persistent(),
        contract: contractId,
        key,
      }),
    );
  }
}
