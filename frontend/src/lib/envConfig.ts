import { StrKey } from "@stellar/stellar-sdk";
import { rpcUrlError } from "./stellar";
import type { NetworkConfig } from "./types";

/** Environment variables the app reads once, at startup. */
export const ENV_VARS = {
  contractId: "VITE_STELLAR_CONTRACT_ID",
  rpcUrl: "VITE_STELLAR_RPC_URL",
  networkPassphrase: "VITE_STELLAR_NETWORK_PASSPHRASE",
} as const;

export const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
export const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";

/**
 * Networks the app recognises. A passphrase that is not on this list is a
 * configuration error: signing against an unknown network is how a user
 * ends up sending transactions that can never succeed.
 */
export const KNOWN_NETWORKS: ReadonlyArray<{
  name: string;
  passphrase: string;
}> = [
  { name: "Testnet", passphrase: TESTNET_PASSPHRASE },
  {
    name: "Public",
    passphrase: "Public Global Stellar Network ; September 2015",
  },
  {
    name: "Futurenet",
    passphrase: "Test SDF Future Network ; October 2022",
  },
];

export type EnvConfigIssue = {
  variable: (typeof ENV_VARS)[keyof typeof ENV_VARS];
  message: string;
};

/** A validated startup configuration, with a human-readable network name. */
export type EnvConfig = NetworkConfig & { networkName: string };

export type EnvConfigResult =
  | { ok: true; config: EnvConfig }
  | { ok: false; issues: EnvConfigIssue[] };

/** Raised in development when the environment is unusable. */
export class EnvConfigError extends Error {
  readonly issues: EnvConfigIssue[];

  constructor(issues: EnvConfigIssue[]) {
    super(formatIssues(issues));
    this.name = "EnvConfigError";
    this.issues = issues;
  }
}

export function formatIssues(issues: EnvConfigIssue[]): string {
  return issues
    .map((issue) => `${issue.variable} ${issue.message}`)
    .join("\n");
}

/** Returns the display name for a known passphrase, or `null`. */
export function networkNameFor(passphrase: string): string | null {
  return (
    KNOWN_NETWORKS.find((network) => network.passphrase === passphrase)
      ?.name ?? null
  );
}

type RawEnv = Partial<Record<string, string | undefined>>;

/**
 * Parses and validates the build-time environment exactly once.
 *
 * Every problem is reported with the variable that caused it, so a broken
 * deployment says which value to fix instead of surfacing as a failed
 * contract call deep in the UI.
 */
export function readEnvConfig(env: RawEnv): EnvConfigResult {
  const issues: EnvConfigIssue[] = [];

  const contractId = env[ENV_VARS.contractId]?.trim() ?? "";
  if (!contractId) {
    issues.push({
      variable: ENV_VARS.contractId,
      message:
        "is not set. Copy frontend/.env.example to frontend/.env and set it to the deployed contract id (a StrKey beginning with C).",
    });
  } else if (!StrKey.isValidContract(contractId)) {
    issues.push({
      variable: ENV_VARS.contractId,
      message: `is not a valid contract id ("${contractId}"). Expected a 56-character StrKey beginning with C.`,
    });
  }

  const networkPassphrase =
    env[ENV_VARS.networkPassphrase]?.trim() || TESTNET_PASSPHRASE;
  const networkName = networkNameFor(networkPassphrase);
  if (networkName === null) {
    const known = KNOWN_NETWORKS.map(
      (network) => `${network.name} ("${network.passphrase}")`,
    ).join(", ");
    issues.push({
      variable: ENV_VARS.networkPassphrase,
      message: `is not a known Stellar network passphrase ("${networkPassphrase}"). Known networks: ${known}.`,
    });
  }

  const rpcUrl = env[ENV_VARS.rpcUrl]?.trim() || DEFAULT_RPC_URL;
  const rpcProblem = rpcUrlError(rpcUrl);
  if (rpcProblem) {
    issues.push({
      variable: ENV_VARS.rpcUrl,
      message: `${rpcProblem} ("${rpcUrl}")`,
    });
  }

  if (issues.length > 0) {
    return { ok: false, issues };
  }

  return {
    ok: true,
    config: {
      contractId,
      rpcUrl,
      networkPassphrase,
      networkName: networkName ?? "Unknown network",
    },
  };
}
