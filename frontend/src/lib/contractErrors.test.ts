import errorsSource from "../../../src/errors.rs?raw";
import {
  CONTRACT_ERROR_COPY,
  formatContractError,
  rawContractError,
} from "./contractErrors";
import { errorMessage } from "./format";

function contractErrorVariants(source: string): Map<string, number> {
  const start = source.indexOf("pub enum ContractError");
  const open = source.indexOf("{", start);
  let depth = 0;
  let end = -1;
  for (let index = open; index < source.length; index += 1) {
    const character = source[index];
    if (character === "{") {
      depth += 1;
    } else if (character === "}") {
      depth -= 1;
      if (depth === 0) {
        end = index;
        break;
      }
    }
  }
  const body = source.slice(open + 1, end);
  const variants = new Map<string, number>();
  for (const line of body.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//")) {
      continue;
    }
    const match = /^([A-Za-z0-9_]+)\s*=\s*(\d+)\s*,/.exec(trimmed);
    if (!match) {
      continue;
    }
    variants.set(match[1], Number(match[2]));
  }
  return variants;
}

describe("contract error catalog", () => {
  const variants = contractErrorVariants(errorsSource);

  it("maps every variant in errors.rs", () => {
    const mapped = new Map(
      CONTRACT_ERROR_COPY.map((entry) => [entry.variant, entry.code]),
    );
    expect([...mapped.keys()].sort()).toEqual([...variants.keys()].sort());
    for (const [name, code] of variants) {
      expect(mapped.get(name)).toBe(code);
    }
  });

  it("renders a specific message and the raw code for every variant", () => {
    for (const entry of CONTRACT_ERROR_COPY) {
      const rendered = errorMessage(
        new Error(`HostError: Error(Contract, #${entry.code})`),
      );
      expect(rendered).toBe(formatContractError(entry));
      expect(rendered).toContain(entry.message);
      expect(rendered).toContain(rawContractError(entry.code));
      expect(rendered).not.toBe("transaction failed");
    }
  });

  it("distinguishes failures the user can act on", () => {
    const byName = new Map(
      CONTRACT_ERROR_COPY.map((entry) => [entry.variant, entry]),
    );
    for (const name of [
      "WrapAlreadyExists",
      "InvalidPeriod",
      "Paused",
      "InvalidMerkleProof",
    ]) {
      const entry = byName.get(name);
      if (!entry) {
        throw new Error(`missing mapping for ${name}`);
      }
      expect(entry.actionable).toBe(true);
      expect(formatContractError(entry)).not.toContain("bug report");
    }
    for (const name of [
      "ArithmeticOverflow",
      "StorageInvariantViolation",
      "Unauthorized",
    ]) {
      const entry = byName.get(name);
      if (!entry) {
        throw new Error(`missing mapping for ${name}`);
      }
      expect(entry.actionable).toBe(false);
      const rendered = formatContractError(entry);
      expect(rendered).toContain("You cannot resolve this");
      expect(rendered).toContain(rawContractError(entry.code));
    }
  });

  it("leaves non-contract failures unchanged and still reports unknown codes", () => {
    expect(errorMessage(new Error("Enter a valid Soroban RPC URL."))).toBe(
      "Enter a valid Soroban RPC URL.",
    );
    expect(errorMessage("transaction failed")).toBe("transaction failed");
    expect(errorMessage({ error: "Error(Contract, #12)" })).toContain(
      "The contract is paused",
    );
    expect(errorMessage(new Error("Error(Contract,#999)"))).toBe(
      "The contract returned an unrecognized error. Include Error(Contract, #999) in a bug report.",
    );
    expect(errorMessage(null)).toBe(
      "Something unexpected happened. Please try again.",
    );
  });
});
