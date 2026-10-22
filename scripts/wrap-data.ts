/**
 * Reference helpers for wrap-data hashing (RFC 8785 + SHA-256).
 *
 * Off-chain parties (frontend, signer, third-party verifiers) MUST:
 *   1. Build a JSON object that includes `schema_version` (see schema/wrap-data.schema.json).
 *   2. Canonicalize with RFC 8785 (JSON Canonicalization Scheme).
 *   3. Take SHA-256 of the UTF-8 canonical bytes → `data_hash` for mint_wrap.
 *   4. Pass those same bytes to verify_data.
 *
 * Usage:
 *   npm install
 *   npx ts-node wrap-data.ts            # verify shared vectors
 *   npx ts-node wrap-data.ts --hash '{"schema_version":1,"score":100}'
 */

import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import canonicalize from "canonicalize";

/** Must match `schema_version` const in schema/wrap-data.schema.json. */
export const CURRENT_WRAP_DATA_SCHEMA_VERSION = 1;

export type WrapData = {
  schema_version: number;
  [key: string]: unknown;
};

/**
 * RFC 8785 JSON Canonicalization Scheme.
 * Returns the deterministic UTF-8 string, or throws if the value is not JSON-serializable.
 */
export function canonicalizeWrapData(value: unknown): string {
  const out = canonicalize(value);
  if (out === undefined) {
    throw new Error("canonicalize returned undefined (non-JSON value)");
  }
  return out;
}

/** SHA-256 of UTF-8 bytes, as a 32-byte Buffer. */
export function sha256Utf8(canonical: string): Buffer {
  return createHash("sha256").update(canonical, "utf8").digest();
}

/** Full pipeline: object → JCS bytes → data_hash. */
export function dataHashOf(value: unknown): Buffer {
  return sha256Utf8(canonicalizeWrapData(value));
}

export type Vector = {
  name: string;
  input: unknown;
  jcs: string;
  sha256: string;
};

function hex(buf: Buffer): string {
  return buf.toString("hex");
}

function loadVectors(): Vector[] {
  const vectorsPath = path.join(__dirname, "..", "schema", "vectors.json");
  const raw = JSON.parse(fs.readFileSync(vectorsPath, "utf8")) as {
    vectors: Vector[];
  };
  return raw.vectors;
}

/** Assert every shared vector matches this reference implementation. */
export function verifySharedVectors(): void {
  const vectors = loadVectors();
  for (const v of vectors) {
    const jcs = canonicalizeWrapData(v.input);
    const digest = hex(sha256Utf8(jcs));
    if (jcs !== v.jcs) {
      throw new Error(
        `vector "${v.name}": JCS mismatch\n  got:  ${jcs}\n  want: ${v.jcs}`,
      );
    }
    if (digest !== v.sha256) {
      throw new Error(
        `vector "${v.name}": sha256 mismatch\n  got:  ${digest}\n  want: ${v.sha256}`,
      );
    }
  }
  console.log(`ok: ${vectors.length} wrap-data vectors match RFC 8785 + SHA-256`);
}

function main(): void {
  const args = process.argv.slice(2);
  if (args[0] === "--hash" && args[1]) {
    const value = JSON.parse(args[1]);
    const jcs = canonicalizeWrapData(value);
    const digest = hex(sha256Utf8(jcs));
    console.log(JSON.stringify({ jcs, sha256: digest }, null, 2));
    return;
  }
  verifySharedVectors();
}

if (require.main === module) {
  main();
}
