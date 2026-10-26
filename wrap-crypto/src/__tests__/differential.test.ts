import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Keypair } from "@stellar/stellar-sdk";
import { describe, expect, it } from "vitest";
import { buildBatchMintPayload, buildMintPayload } from "../payload";

interface MintVector {
  name: string;
  payload_version: number;
  contract_id: string;
  user: string;
  period: number;
  archetype: string;
  data_hash_hex: string;
  valid_until: string;
  payload_hex: string;
  signature_hex: string;
}

interface BatchItemVector {
  user: string;
  period: number;
  archetype: string;
  data_hash_hex: string;
}

interface BatchVector {
  name: string;
  payload_version: number;
  contract_id: string;
  items: BatchItemVector[];
  payload_hex: string;
  signature_hex: string;
}

interface VectorsFile {
  secret_seed_hex: string;
  pubkey_hex: string;
  mint: MintVector[];
  batch: BatchVector[];
}

function loadVectors(): VectorsFile {
  const path = join(__dirname, "../../vectors.json");
  return JSON.parse(readFileSync(path, "utf8")) as VectorsFile;
}

function hex(buf: Buffer): string {
  return buf.toString("hex");
}

describe("Rust ↔ TypeScript payload differential", () => {
  const vectors = loadVectors();
  const keypair = Keypair.fromRawEd25519Seed(
    Buffer.from(vectors.secret_seed_hex, "hex"),
  );

  it("covers at least one mint and one batch vector", () => {
    expect(vectors.mint.length).toBeGreaterThan(0);
    expect(vectors.batch.length).toBeGreaterThan(0);
  });

  it("fixture pubkey matches the secret seed", () => {
    expect(hex(keypair.rawPublicKey())).toBe(vectors.pubkey_hex);
  });

  for (const v of vectors.mint) {
    it(`${v.name}: TS payload bytes equal Rust vector`, () => {
      const rebuilt = buildMintPayload({
        contractId: v.contract_id,
        user: v.user,
        period: BigInt(v.period),
        archetype: v.archetype,
        dataHash: Buffer.from(v.data_hash_hex, "hex"),
        payloadVersion: v.payload_version,
        validUntil: BigInt(v.valid_until),
      });

      expect(hex(rebuilt)).toBe(v.payload_hex);
      expect(rebuilt.equals(Buffer.from(v.payload_hex, "hex"))).toBe(true);
    });

    it(`${v.name}: Rust signature verifies over TS payload`, () => {
      const rebuilt = buildMintPayload({
        contractId: v.contract_id,
        user: v.user,
        period: BigInt(v.period),
        archetype: v.archetype,
        dataHash: Buffer.from(v.data_hash_hex, "hex"),
        payloadVersion: v.payload_version,
        validUntil: BigInt(v.valid_until),
      });
      const signature = Buffer.from(v.signature_hex, "hex");
      expect(keypair.verify(rebuilt, signature)).toBe(true);
    });
  }

  for (const v of vectors.batch) {
    it(`${v.name}: TS payload bytes equal Rust vector`, () => {
      const rebuilt = buildBatchMintPayload({
        contractId: v.contract_id,
        payloadVersion: v.payload_version,
        items: v.items.map((item) => ({
          user: item.user,
          period: BigInt(item.period),
          archetype: item.archetype,
          dataHash: Buffer.from(item.data_hash_hex, "hex"),
        })),
      });

      expect(hex(rebuilt)).toBe(v.payload_hex);
      expect(rebuilt.equals(Buffer.from(v.payload_hex, "hex"))).toBe(true);
    });

    it(`${v.name}: Rust signature verifies over TS payload`, () => {
      const rebuilt = buildBatchMintPayload({
        contractId: v.contract_id,
        payloadVersion: v.payload_version,
        items: v.items.map((item) => ({
          user: item.user,
          period: BigInt(item.period),
          archetype: item.archetype,
          dataHash: Buffer.from(item.data_hash_hex, "hex"),
        })),
      });
      const signature = Buffer.from(v.signature_hex, "hex");
      expect(keypair.verify(rebuilt, signature)).toBe(true);
    });
  }

  it("fails closed when a single payload byte differs", () => {
    const v = vectors.mint[0];
    const rebuilt = buildMintPayload({
      contractId: v.contract_id,
      user: v.user,
      period: BigInt(v.period),
      archetype: v.archetype,
      dataHash: Buffer.from(v.data_hash_hex, "hex"),
      payloadVersion: v.payload_version,
      validUntil: BigInt(v.valid_until),
    });
    const tampered = Buffer.from(rebuilt);
    tampered[0] ^= 0x01;
    expect(hex(tampered)).not.toBe(v.payload_hex);
    expect(
      keypair.verify(tampered, Buffer.from(v.signature_hex, "hex")),
    ).toBe(false);
  });
});
