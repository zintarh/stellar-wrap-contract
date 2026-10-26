import {
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  nativeToScVal,
  rpc,
  scValToNative,
  StrKey,
  Transaction,
  TransactionBuilder,
} from "@stellar/stellar-sdk";
import { normalizeHealth, normalizeWrap, normalizeWrapView } from "./format";
import { signWithFreighter } from "./freighter";
import type {
  ContractHealth,
  Dashboard,
  MintInput,
  NetworkConfig,
  WrapRecord,
  WrapView,
} from "./types";

const TRANSACTION_TIMEOUT_SECONDS = 30;

/**
 * The contract interface the frontend depends on. This is the single
 * source of truth for the function names, argument order, and argument
 * types that the frontend bindings expect. The test suite asserts this
 * spec against the built contract's spec (WASM custom section / XDR) so a
 * breaking contract-interface change fails CI instead of surfacing in the
 * browser.
 */
export interface ContractFunctionSpec {
  name: string;
  args: Array<{ type: string }>;
  returns: string;
}

export const CONTRACT_INTERFACE: ContractFunctionSpec[] = [
  { name: "health", args: [], returns: "map" },
  {
    name: "balance_of",
    args: [{ type: "address" }],
    returns: "i128",
  },
  {
    name: "get_latest_wrap",
    args: [{ type: "address" }],
    returns: "option",
  },
  {
    name: "get_wrap",
    args: [{ type: "address" }, { type: "u64" }],
    returns: "option",
  },
  {
    name: "get_wrap_v2",
    args: [{ type: "address" }, { type: "u64" }],
    returns: "option",
  },
  {
    name: "get_wraps_v2",
    args: [
      { type: "address" },
      { type: "u32" },
      { type: "u32" },
    ],
    returns: "vec",
  },
  {
    name: "mint_wrap",
    args: [
      { type: "address" },
      { type: "u64" },
      { type: "symbol" },
      { type: "bytes" },
      { type: "bytes" },
    ],
    returns: "bytes",
  },
];

function isLoopbackHost(hostname: string): boolean {
  return (
    hostname === "localhost" ||
    hostname === "127.0.0.1" ||
    hostname === "[::1]"
  );
}

export function validateConfig(config: NetworkConfig): NetworkConfig {
  const contractId = config.contractId.trim();
  const rpcUrl = config.rpcUrl.trim();
  const networkPassphrase = config.networkPassphrase.trim();

  if (!StrKey.isValidContract(contractId)) {
    throw new Error("Enter a valid Stellar contract ID beginning with C.");
  }

  let url: URL;
  try {
    url = new URL(rpcUrl);
  } catch {
    throw new Error("Enter a valid Soroban RPC URL.");
  }
  const isLocalHttp =
    url.protocol === "http:" && isLoopbackHost(url.hostname);
  if (url.protocol !== "https:" && !isLocalHttp) {
    throw new Error("The RPC URL must use HTTPS (except on localhost).");
  }
  if (url.username || url.password) {
    throw new Error("The RPC URL must not contain embedded credentials.");
  }
  if (!networkPassphrase) {
    throw new Error("Network passphrase is required.");
  }

  return { contractId, rpcUrl, networkPassphrase };
}

function makeServer(config: NetworkConfig): rpc.Server {
  return new rpc.Server(config.rpcUrl, {
    allowHttp: isLoopbackHost(new URL(config.rpcUrl).hostname),
  });
}

function simulationError(simulation: rpc.Api.SimulateTransactionResponse): Error {
  if (rpc.Api.isSimulationError(simulation)) {
    return new Error(`Contract simulation failed: ${simulation.error}`);
  }
  return new Error("Contract simulation returned no result.");
}

async function buildTransaction(
  config: NetworkConfig,
  source: string,
  method: string,
  args: ReturnType<typeof nativeToScVal>[],
) {
  const server = makeServer(config);
  const account = await server.getAccount(source);
  const contract = new Contract(config.contractId);
  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: config.networkPassphrase,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(TRANSACTION_TIMEOUT_SECONDS)
    .build();

  return { server, transaction };
}

async function readContract(
  config: NetworkConfig,
  source: string,
  method: string,
  args: ReturnType<typeof nativeToScVal>[] = [],
): Promise<unknown> {
  const { server, transaction } = await buildTransaction(
    config,
    source,
    method,
    args,
  );
  const simulation = await server.simulateTransaction(transaction);

  if (!rpc.Api.isSimulationSuccess(simulation) || !simulation.result) {
    throw simulationError(simulation);
  }
  return scValToNative(simulation.result.retval);
}

export async function loadDashboard(
  config: NetworkConfig,
  address: string,
): Promise<Dashboard> {
  const addressArg = nativeToScVal(Address.fromString(address));
  const [health, balance, latestWrap, wraps] = await Promise.all([
    readContract(config, address, "health"),
    readContract(config, address, "balance_of", [addressArg]),
    readContract(config, address, "get_latest_wrap", [addressArg]),
    readContract(config, address, "get_all_wraps_for_user", [addressArg]),
  ]);
  if (!Array.isArray(wraps)) {
    throw new Error("The contract returned invalid wrap records.");
  }
  const records = wraps.map((wrap) => {
    const record = normalizeWrap(wrap);
    if (!record) {
      throw new Error("The contract returned an empty wrap record.");
    }
    return record;
  });

  return {
    health: normalizeHealth(health),
    balance: BigInt(balance as bigint | number | string),
    latestWrap: normalizeWrap(latestWrap),
    records,
  };
}

export async function getWrap(
  config: NetworkConfig,
  address: string,
  period: bigint,
): Promise<WrapRecord | null> {
  const value = await readContract(config, address, "get_wrap", [
    nativeToScVal(Address.fromString(address)),
    nativeToScVal(period, { type: "u64" }),
  ]);
  return normalizeWrap(value);
}

export async function getWrapV2(
  config: NetworkConfig,
  address: string,
  period: bigint,
): Promise<WrapView | null> {
  const value = await readContract(config, address, "get_wrap_v2", [
    nativeToScVal(Address.fromString(address)),
    nativeToScVal(period, { type: "u64" }),
  ]);
  return normalizeWrapView(value);
}

export async function getWrapsV2(
  config: NetworkConfig,
  address: string,
  start: number,
  limit: number,
): Promise<WrapView[]> {
  const value = await readContract(config, address, "get_wraps_v2", [
    nativeToScVal(Address.fromString(address)),
    nativeToScVal(start, { type: "u32" }),
    nativeToScVal(limit, { type: "u32" }),
  ]);
  if (!Array.isArray(value)) {
    throw new Error("The contract returned invalid wrap views.");
  }
  return value.map((wrap) => {
    const view = normalizeWrapView(wrap);
    if (!view) {
      throw new Error("The contract returned an empty wrap view.");
    }
    return view;
  });
}

export async function getContractHealth(
  config: NetworkConfig,
  address: string,
): Promise<ContractHealth> {
  return normalizeHealth(await readContract(config, address, "health"));
}

export async function mintWrap(
  config: NetworkConfig,
  address: string,
  input: MintInput,
): Promise<string> {
  const args = [
    nativeToScVal(Address.fromString(address)),
    nativeToScVal(input.period, { type: "u64" }),
    nativeToScVal(input.archetype, { type: "symbol" }),
    nativeToScVal(input.dataHash, { type: "bytes" }),
    nativeToScVal(input.signature, { type: "bytes" }),
  ];
  const { server, transaction } = await buildTransaction(
    config,
    address,
    "mint_wrap",
    args,
  );
  const simulation = await server.simulateTransaction(transaction);
  if (!rpc.Api.isSimulationSuccess(simulation)) {
    throw simulationError(simulation);
  }

  const prepared = rpc.assembleTransaction(transaction, simulation).build();
  const signedXdr = await signWithFreighter(
    prepared.toXDR(),
    address,
    config.networkPassphrase,
  );
  const signed = new Transaction(signedXdr, config.networkPassphrase);
  const submission = await server.sendTransaction(signed);

  if (submission.status !== "PENDING" && submission.status !== "DUPLICATE") {
    throw new Error(`Transaction submission failed with status ${submission.status}.`);
  }

  const result = await server.pollTransaction(submission.hash, {
    attempts: 30,
    sleepStrategy: () => 1000,
  });
  if (result.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`Transaction was not confirmed: ${result.status}.`);
  }

  return submission.hash;
}

/**
 * Options for a sponsor-pays mint.
 *
 * The sponsor covers transaction fees and storage rent on behalf of the user.
 * The user must still sign the inner transaction (Freighter) to satisfy
 * `user.require_auth()` inside the contract.
 */
export type SponsoredMintInput = MintInput & {
  /**
   * The sponsor's Stellar keypair. The sponsor is the fee-bump source account
   * and pays all fees. The sponsor has no contract authority.
   */
  sponsorKeypair: Keypair;
  /**
   * Maximum fee the sponsor is willing to pay for the outer fee-bump envelope,
   * in stroops (1 XLM = 10,000,000 stroops). Defaults to 1,000,000 stroops
   * (0.1 XLM), which is well above the typical fee for a single Soroban call.
   */
  maxFeeSroops?: string;
};

/**
 * Mint a wrap record where a sponsor pays the transaction fee and storage rent.
 *
 * Flow:
 *  1. Build the inner `mint_wrap` transaction with the user as the source account.
 *  2. Simulate to assemble Soroban auth entries.
 *  3. Sign the inner transaction with Freighter (satisfies user.require_auth()).
 *  4. Wrap the signed inner transaction in a fee-bump envelope signed by the sponsor.
 *  5. Submit the fee-bump transaction and poll for confirmation.
 *
 * The user's account is never charged fees. The sponsor's account covers both
 * the network fee and the persistent storage rent for the new wrap record.
 *
 * @param config  - Network and contract configuration.
 * @param address - User's Stellar address (the account that will own the wrap).
 * @param input   - Mint parameters plus the sponsor keypair.
 * @returns       The confirmed transaction hash.
 *
 * @example
 * ```ts
 * import { Keypair } from "@stellar/stellar-sdk";
 * import { mintWrapSponsored } from "./stellar";
 *
 * const sponsorKeypair = Keypair.fromSecret(import.meta.env.VITE_SPONSOR_SECRET!);
 *
 * const txHash = await mintWrapSponsored(config, wallet.address, {
 *   period: 202406n,
 *   archetype: "builder",
 *   dataHash: parseHexBytes(dataHash, 32, "Data hash"),
 *   signature: parseHexBytes(adminSignature, 64, "Admin signature"),
 *   sponsorKeypair,
 * });
 * ```
 */
export async function mintWrapSponsored(
  config: NetworkConfig,
  address: string,
  input: SponsoredMintInput,
): Promise<string> {
  const maxFee = input.maxFeeSroops ?? "1000000";

  // Step 1 & 2: build and simulate the inner transaction
  const args = [
    nativeToScVal(Address.fromString(address)),
    nativeToScVal(input.period, { type: "u64" }),
    nativeToScVal(input.archetype, { type: "symbol" }),
    nativeToScVal(input.dataHash, { type: "bytes" }),
    nativeToScVal(1, { type: "u32" }), // payload_version
    nativeToScVal(input.signature, { type: "bytes" }),
  ];

  const { server, transaction } = await buildTransaction(
    config,
    address,
    "mint_wrap",
    args,
  );

  const simulation = await server.simulateTransaction(transaction);
  if (!rpc.Api.isSimulationSuccess(simulation)) {
    throw simulationError(simulation);
  }

  // Assemble Soroban auth entries and fee padding from simulation
  const innerPrepared = rpc.assembleTransaction(transaction, simulation).build();

  // Step 3: user signs the inner transaction via Freighter
  const signedInnerXdr = await signWithFreighter(
    innerPrepared.toXDR(),
    address,
    config.networkPassphrase,
  );
  const signedInner = new Transaction(signedInnerXdr, config.networkPassphrase);

  // Step 4: sponsor wraps the signed inner transaction in a fee-bump envelope
  const feeBump = TransactionBuilder.buildFeeBumpTransaction(
    input.sponsorKeypair,
    maxFee,
    signedInner,
    config.networkPassphrase,
  );

  feeBump.sign(input.sponsorKeypair);

  // Step 5: submit and poll
  const submission = await server.sendTransaction(feeBump);
  if (submission.status !== "PENDING" && submission.status !== "DUPLICATE") {
    throw new Error(
      `Fee-bump submission failed with status ${submission.status}.`,
    );
  }

  const result = await server.pollTransaction(submission.hash, {
    attempts: 30,
    sleepStrategy: () => 1000,
  });
  if (result.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(
      `Sponsored mint transaction was not confirmed: ${result.status}.`,
    );
  }

  return submission.hash;
}
