import { Address, nativeToScVal, xdr } from "@stellar/stellar-sdk";

/** Raw ASCII — must match `MINT_DOMAIN_SEPARATOR` in `src/signature.rs`. */
export const MINT_DOMAIN_SEPARATOR = Buffer.from("stellar-wrap-v1", "ascii");

/** Raw ASCII — must match `BATCH_MINT_DOMAIN_SEPARATOR` in `src/signature.rs`. */
export const BATCH_MINT_DOMAIN_SEPARATOR = Buffer.from(
  "stellar-wrap-batch-v1",
  "ascii",
);

export interface MintClaim {
  contractId: string;
  user: string;
  period: bigint;
  archetype: string;
  dataHash: Buffer;
  payloadVersion: number;
  validUntil: bigint;
}

export interface BatchMintItem {
  user: string;
  period: bigint;
  archetype: string;
  dataHash: Buffer;
}

export interface BatchMintClaim {
  contractId: string;
  payloadVersion: number;
  items: BatchMintItem[];
}

/**
 * Builds the exact mint payload bytes the contract verifies.
 *
 * Matches `construct_mint_payload`: raw domain separator ‖ XDR(`MintPayload`).
 * Map keys are the Rust field names (snake_case), sorted by symbol ASCII order.
 */
export function buildMintPayload(claim: MintClaim): Buffer {
  const archetypeSc = nativeToScVal(claim.archetype, { type: "symbol" });
  const contractSc = Address.fromString(claim.contractId).toScVal();
  const hashSc = nativeToScVal(claim.dataHash, { type: "bytes" });
  const versionSc = nativeToScVal(claim.payloadVersion, { type: "u32" });
  const periodSc = nativeToScVal(claim.period, { type: "u64" });
  const userSc = Address.fromString(claim.user).toScVal();
  const untilSc = nativeToScVal(claim.validUntil, { type: "u64" });

  const structFields = xdr.ScVal.scvMap([
    new xdr.ScMapEntry({
      key: nativeToScVal("archetype", { type: "symbol" }),
      val: archetypeSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("contract_id", { type: "symbol" }),
      val: contractSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("data_hash", { type: "symbol" }),
      val: hashSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("payload_version", { type: "symbol" }),
      val: versionSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("period", { type: "symbol" }),
      val: periodSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("user", { type: "symbol" }),
      val: userSc,
    }),
    new xdr.ScMapEntry({
      key: nativeToScVal("valid_until", { type: "symbol" }),
      val: untilSc,
    }),
  ]);

  return Buffer.concat([MINT_DOMAIN_SEPARATOR, structFields.toXDR()]);
}

/**
 * Builds the exact batch mint payload bytes the contract verifies.
 *
 * Matches `construct_batch_mint_payload`:
 * domain ‖ XDR(version) ‖ XDR(contract) ‖ XDR(len) ‖ Σ item fields.
 */
export function buildBatchMintPayload(claim: BatchMintClaim): Buffer {
  const parts: Buffer[] = [
    BATCH_MINT_DOMAIN_SEPARATOR,
    nativeToScVal(claim.payloadVersion, { type: "u32" }).toXDR(),
    Address.fromString(claim.contractId).toScVal().toXDR(),
    nativeToScVal(claim.items.length, { type: "u32" }).toXDR(),
  ];

  for (const item of claim.items) {
    parts.push(Address.fromString(item.user).toScVal().toXDR());
    parts.push(nativeToScVal(item.period, { type: "u64" }).toXDR());
    parts.push(nativeToScVal(item.archetype, { type: "symbol" }).toXDR());
    parts.push(nativeToScVal(item.dataHash, { type: "bytes" }).toXDR());
  }

  return Buffer.concat(parts);
}
