import { describe, expect, it } from "vitest";
import {
  DEFAULT_RPC_URL,
  ENV_VARS,
  EnvConfigError,
  KNOWN_NETWORKS,
  TESTNET_PASSPHRASE,
  formatIssues,
  networkNameFor,
  readEnvConfig,
} from "./envConfig";
import type { EnvConfigResult } from "./envConfig";

const VALID_CONTRACT_ID =
  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

function issueVariables(result: EnvConfigResult): string[] {
  if (result.ok) {
    throw new Error("expected the environment to be rejected");
  }
  return result.issues.map((issue) => issue.variable);
}

describe("readEnvConfig", () => {
  it("names the variable when the contract id is missing", () => {
    const result = readEnvConfig({});
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([ENV_VARS.contractId]);
    if (!result.ok) {
      expect(result.issues[0].message).toContain(".env.example");
    }
  });

  it("names the variable when the contract id is malformed", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: "not-a-contract-id",
    });
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([ENV_VARS.contractId]);
    if (!result.ok) {
      expect(result.issues[0].message).toContain("not-a-contract-id");
    }
  });

  it("defaults to testnet when only the contract id is set", () => {
    const result = readEnvConfig({ [ENV_VARS.contractId]: VALID_CONTRACT_ID });
    expect(result).toEqual({
      ok: true,
      config: {
        contractId: VALID_CONTRACT_ID,
        rpcUrl: DEFAULT_RPC_URL,
        networkPassphrase: TESTNET_PASSPHRASE,
        networkName: "Testnet",
      },
    });
  });

  it("names the variable when the passphrase is not a known network", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: VALID_CONTRACT_ID,
      [ENV_VARS.networkPassphrase]: "Stellar Test Network (made up)",
    });
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([ENV_VARS.networkPassphrase]);
    if (!result.ok) {
      for (const network of KNOWN_NETWORKS) {
        expect(result.issues[0].message).toContain(network.passphrase);
      }
    }
  });

  it("names the variable when the RPC URL is malformed", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: VALID_CONTRACT_ID,
      [ENV_VARS.rpcUrl]: "not a url",
    });
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([ENV_VARS.rpcUrl]);
  });

  it("names the variable when the RPC URL is insecure", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: VALID_CONTRACT_ID,
      [ENV_VARS.rpcUrl]: "http://rpc.example.com",
    });
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([ENV_VARS.rpcUrl]);
  });

  it("accepts plain HTTP on localhost", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: VALID_CONTRACT_ID,
      [ENV_VARS.rpcUrl]: "http://127.0.0.1:8000",
    });
    expect(result.ok).toBe(true);
  });

  it("reports every unusable variable at once", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: "bad",
      [ENV_VARS.networkPassphrase]: "unknown",
      [ENV_VARS.rpcUrl]: "ftp://rpc.example.com",
    });
    expect(result.ok).toBe(false);
    expect(issueVariables(result)).toEqual([
      ENV_VARS.contractId,
      ENV_VARS.networkPassphrase,
      ENV_VARS.rpcUrl,
    ]);
  });

  it("trims surrounding whitespace from supplied values", () => {
    const result = readEnvConfig({
      [ENV_VARS.contractId]: `  ${VALID_CONTRACT_ID}  `,
      [ENV_VARS.rpcUrl]: "  https://rpc.example.com  ",
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.config.contractId).toBe(VALID_CONTRACT_ID);
      expect(result.config.rpcUrl).toBe("https://rpc.example.com");
    }
  });
});

describe("networkNameFor", () => {
  it("maps every known passphrase to its display name", () => {
    for (const network of KNOWN_NETWORKS) {
      expect(networkNameFor(network.passphrase)).toBe(network.name);
    }
  });

  it("returns null for an unknown passphrase", () => {
    expect(networkNameFor("not a network")).toBeNull();
  });
});

describe("EnvConfigError", () => {
  it("names every offending variable in its message", () => {
    const result = readEnvConfig({});
    if (result.ok) {
      throw new Error("expected the environment to be rejected");
    }
    const error = new EnvConfigError(result.issues);
    expect(error.name).toBe("EnvConfigError");
    expect(error.message).toContain(ENV_VARS.contractId);
    expect(error.issues).toEqual(result.issues);
  });

  it("formats issues as `variable message` lines", () => {
    expect(
      formatIssues([
        { variable: ENV_VARS.contractId, message: "is not set." },
        { variable: ENV_VARS.rpcUrl, message: "is not a URL." },
      ]),
    ).toBe(
      `${ENV_VARS.contractId} is not set.\n${ENV_VARS.rpcUrl} is not a URL.`,
    );
  });
});
