import { createHash } from "crypto";
import { basename } from "path";
import { Address, nativeToScVal, xdr } from "@stellar/stellar-sdk";

export type ClaimLeaf = {
  user: string;
  period: bigint;
  archetype: string;
  dataHash: Buffer;
};

const MAX_PROOF_DEPTH = 32;

function sha256(value: Buffer): Buffer {
  return createHash("sha256").update(value).digest();
}

function toXdrBytes(value: xdr.ScVal): Buffer {
  return Buffer.from(value.toXDR());
}

export function encodeMerkleLeaf(leaf: ClaimLeaf, _networkPassphrase: string): Buffer {
  const userAddress = Address.fromString(leaf.user);
  return sha256(Buffer.concat([
    Buffer.from([0x00]),
    toXdrBytes(userAddress.toScVal()),
    toXdrBytes(nativeToScVal(leaf.period, { type: "u64" })),
    toXdrBytes(nativeToScVal(leaf.archetype, { type: "symbol" })),
    toXdrBytes(nativeToScVal(leaf.dataHash, { type: "bytes" })),
  ]));
}

function hashPair(a: Buffer, b: Buffer): Buffer {
  const [left, right] = Buffer.compare(a, b) <= 0 ? [a, b] : [b, a];
  return sha256(Buffer.concat([Buffer.from([0x01]), left, right]));
}

export function buildMerkleRoot(leaves: Buffer[]): Buffer {
  if (leaves.length === 0) throw new Error("empty tree");
  let layer: Buffer<ArrayBufferLike>[] = leaves.map((leaf) => Buffer.from(leaf));
  while (layer.length > 1) {
    const next: Buffer[] = [];
    for (let index = 0; index < layer.length; index += 2) {
      next.push(index + 1 < layer.length ? hashPair(layer[index], layer[index + 1]) : layer[index]);
    }
    layer = next;
  }
  return layer[0];
}

export function buildMerkleProof(leaves: Buffer[], index: number): Buffer[] {
  if (leaves.length === 0 || index < 0 || index >= leaves.length) {
    throw new Error("invalid leaf index");
  }
  const proof: Buffer[] = [];
  let currentIndex = index;
  let layer: Buffer<ArrayBufferLike>[] = leaves.map((leaf) => Buffer.from(leaf));
  while (layer.length > 1) {
    const siblingIndex = currentIndex % 2 === 0 ? currentIndex + 1 : currentIndex - 1;
    if (siblingIndex < layer.length) {
      proof.push(layer[siblingIndex]);
      if (proof.length > MAX_PROOF_DEPTH) {
        throw new Error("proof depth exceeds MAX_PROOF_DEPTH");
      }
    }
    const next: Buffer[] = [];
    for (let cursor = 0; cursor < layer.length; cursor += 2) {
      next.push(cursor + 1 < layer.length ? hashPair(layer[cursor], layer[cursor + 1]) : layer[cursor]);
    }
    currentIndex = Math.floor(currentIndex / 2);
    layer = next;
  }
  return proof;
}

export function buildClaimTree(
  claims: ClaimLeaf[],
  networkPassphrase: string,
): { root: Buffer; proofs: Buffer[][] } {
  const leaves = claims.map((claim) => encodeMerkleLeaf(claim, networkPassphrase));
  const root = buildMerkleRoot(leaves);
  return { root, proofs: leaves.map((_, index) => buildMerkleProof(leaves, index)) };
}

if (basename(process.argv[1] ?? "") === "merkle.ts") {
  const demo: ClaimLeaf[] = [{
    user: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    period: 202512n,
    archetype: "builder",
    dataHash: Buffer.alloc(32, 1),
  }];
  const { root, proofs } = buildClaimTree(demo, "Test SDF Network ; September 2015");
  console.log("root:", root.toString("hex"));
  console.log("proof depth:", proofs[0].length);
}
