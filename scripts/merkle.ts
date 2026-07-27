/**
 * Merkle tree helper for Stellar Wrap batch claims.
 *
 * Leaf encoding (must match on-chain `compute_merkle_leaf`):
 *   SHA-256( XDR(user) || XDR(period) || XDR(archetype) || XDR(data_hash) )
 *
 * Internal nodes:
 *   SHA-256( min(left,right) || max(left,right) )  — lexicographic byte order
 *
 * Usage:
 *   npm install @stellar/stellar-sdk @noble/hashes
 *   npx ts-node scripts/merkle.ts
 */

import { createHash } from "crypto";
import {
  Address,
  xdr,
  scValToNative,
  nativeToScVal,
} from "@stellar/stellar-sdk";

export type ClaimLeaf = {
  user: string;
  period: bigint;
  archetype: string;
  dataHash: Buffer; // 32 bytes
};

function sha256(buf: Buffer): Buffer {
  return createHash("sha256").update(buf).digest();
}

function toXdrBytes(val: xdr.ScVal): Buffer {
  return Buffer.from(val.toXDR());
}

/** Encode a single merkle leaf exactly as the Soroban contract does. */
export function encodeMerkleLeaf(leaf: ClaimLeaf, networkPassphrase: string): Buffer {
  const userAddr = Address.fromString(leaf.user);
  const parts = [
    toXdrBytes(userAddr.toScVal()),
    toXdrBytes(nativeToScVal(leaf.period, { type: "u64" })),
    toXdrBytes(nativeToScVal(leaf.archetype, { type: "symbol" })),
    toXdrBytes(nativeToScVal(leaf.dataHash, { type: "bytes" })),
  ];
  return sha256(Buffer.concat(parts));
}

function hashPair(a: Buffer, b: Buffer): Buffer {
  const [left, right] = Buffer.compare(a, b) <= 0 ? [a, b] : [b, a];
  return sha256(Buffer.concat([left, right]));
}

/** Build a binary merkle root from pre-encoded 32-byte leaves. */
export function buildMerkleRoot(leaves: Buffer[]): Buffer {
  if (leaves.length === 0) throw new Error("empty tree");
  let layer = leaves.map((l) => Buffer.from(l));
  while (layer.length > 1) {
    const next: Buffer[] = [];
    for (let i = 0; i < layer.length; i += 2) {
      if (i + 1 < layer.length) {
        next.push(hashPair(layer[i], layer[i + 1]));
      } else {
        next.push(layer[i]);
      }
    }
    layer = next;
  }
  return layer[0];
}

/** Generate a proof for `index` in the leaf array. */
export function buildMerkleProof(leaves: Buffer[], index: number): Buffer[] {
  const proof: Buffer[] = [];
  let idx = index;
  let layer = leaves.map((l) => Buffer.from(l));
  while (layer.length > 1) {
    const siblingIdx = idx % 2 === 0 ? idx + 1 : idx - 1;
    if (siblingIdx < layer.length) {
      proof.push(layer[siblingIdx]);
    }
    const next: Buffer[] = [];
    for (let i = 0; i < layer.length; i += 2) {
      if (i + 1 < layer.length) {
        next.push(hashPair(layer[i], layer[i + 1]));
      } else {
        next.push(layer[i]);
      }
    }
    idx = Math.floor(idx / 2);
    layer = next;
  }
  return proof;
}

/** Convenience: encode leaves, return root and per-index proofs. */
export function buildClaimTree(
  claims: ClaimLeaf[],
  networkPassphrase: string
): { root: Buffer; proofs: Buffer[][] } {
  const leaves = claims.map((c) => encodeMerkleLeaf(c, networkPassphrase));
  const root = buildMerkleRoot(leaves);
  const proofs = leaves.map((_, i) => buildMerkleProof(leaves, i));
  return { root, proofs };
}

if (require.main === module) {
  const demo: ClaimLeaf[] = [
    {
      user: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
      period: 202512n,
      archetype: "builder",
      dataHash: Buffer.alloc(32, 1),
    },
  ];
  const { root, proofs } = buildClaimTree(demo, "Test SDF Network ; September 2015");
  console.log("root:", root.toString("hex"));
  console.log("proof depth:", proofs[0].length);
}
