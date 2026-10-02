import {
  Account,
  Contract,
  nativeToScVal,
  rpc,
  scValToNative,
  type Transaction,
  xdr,
} from "@stellar/stellar-sdk";
import { signWithFreighter } from "./freighter";
import {
  getWrap,
  loadDashboard,
  mintWrap,
  validateConfig,
} from "./stellar";
import type { NetworkConfig } from "./types";

const mocks = vi.hoisted(() => {
  const server = {
    getAccount: vi.fn(),
    pollTransaction: vi.fn(),
    sendTransaction: vi.fn(),
    simulateTransaction: vi.fn(),
  };

  return {
    assembleTransaction: vi.fn(),
    contractSpec: vi.fn(),
    server,
    serverConstructor: vi.fn(),
    signWithFreighter: vi.fn(),
  };
});

vi.mock("./freighter", () => ({
  signWithFreighter: mocks.signWithFreighter,
}));

vi.mock("@stellar/stellar-sdk", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@stellar/stellar-sdk")>();

  class MockServer {
    constructor(url: string, options: { allowHttp: boolean }) {
      mocks.serverConstructor(url, options);
      return mocks.server;
    }
  }

  return {
    ...actual,
    Contract: class MockContract {
      constructor(contractId: string) {
        mocks.contractSpec(contractId);
      }
      call(method: string, ...args: xdr.ScVal[]) {
        return actual.Contract.prototype.call.call(
          Object.create(actual.Contract.prototype),
          method,
          ...args,
        );
      }
    },
    rpc: {
      ...actual.rpc,
      assembleTransaction: mocks.assembleTransaction,
      Server: MockServer,
    },
  };
});

const VALID_CONTRACT =
  "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const ADDRESS = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const PASSPHRASE = "Test SDF Network ; September 2015";
const CONFIG: NetworkConfig = {
  contractId: VALID_CONTRACT,
  rpcUrl: "https://soroban-testnet.stellar.org",
  networkPassphrase: PASSPHRASE,
};

const CONTRACT_SPEC_ENTRIES = [
  {
    name: "health",
    inputs: [],
    outputs: ["Health"],
  },
  {
    name: "balance_of",
    inputs: [{ name: "owner", type: "Address" }],
    outputs: ["u64"],
  },
  {
    name: "get_wrap",
    inputs: [
      { name: "owner", type: "Address" },
      { name: "period", type: "u64" },
    ],
    outputs: ["WrapRecord"],
  },
  {
    name: "get_latest_wrap",
    inputs: [{ name: "owner", type: "Address" }],
    outputs: ["WrapRecord"],
  },
  {
    name: "mint_wrap",
    inputs: [
      { name: "owner", type: "Address" },
      { name: "period", type: "u64" },
      { name: "archetype", type: "Symbol" },
      { name: "data_hash", type: "Bytes" },
      { name: "signature", type: "Bytes" },
    ],
    outputs: ["void"],
  },
] as const;

function contractSpec() {
  return {
    funcs: () =>
      CONTRACT_SPEC_ENTRIES.map((entry) => ({
        name: () => entry.name,
        inputs: () =>
          entry.inputs.map((input) => ({
            name: () => input.name,
            type: () => input.type,
          })),
        outputs: () => entry.outputs,
      })),
  };
}

function specEntry(name: string) {
  const entry = CONTRACT_SPEC_ENTRIES.find((candidate) => candidate.name === name);
  if (!entry) {
    throw new Error(`Contract spec is missing ${name}`);
  }
  return entry;
}

function contractInvocation(transaction: Transaction) {
  const operation = transaction.operations[0];
  if (operation.type !== "invokeHostFunction") {
    throw new Error("Expected an invokeHostFunction operation");
  }
  return operation.func.invokeContract();
}

function contractMethod(transaction: Transaction): string {
  return contractInvocation(transaction).functionName().toString();
}

function simulationSuccess(value: xdr.ScVal = xdr.ScVal.scvVoid()) {
  return {
    _parsed: true as const,
    id: "simulation",
    latestLedger: 1,
    events: [],
    minResourceFee: "100",
    transactionData: {},
    result: { auth: [], retval: value },
  };
}

describe("contract RPC adapter", () => {
  beforeEach(() => {
    mocks.contractSpec.mockReturnValue(contractSpec());
    mocks.server.getAccount.mockResolvedValue(new Account(ADDRESS, "1"));
    mocks.server.simulateTransaction.mockResolvedValue(simulationSuccess());
    mocks.server.sendTransaction.mockResolvedValue({
      hash: "transaction-hash",
      status: "PENDING",
    });
    mocks.server.pollTransaction.mockResolvedValue({
      status: rpc.Api.GetTransactionStatus.SUCCESS,
    });
    mocks.assembleTransaction.mockImplementation((transaction: Transaction) => ({
      build: () => transaction,
    }));
    mocks.signWithFreighter.mockImplementation(
      async (transactionXdr: string) => transactionXdr,
    );
  });

it("binds every frontend call to the deployed contract spec", () => {
    const spec = contractSpec();
    const expected = new Map(
      CONTRACT_SPEC_ENTRIES.map((entry) => [entry.name, entry]),
    );

    const invoked = new Set<string>();
    const record = (name: string) => {
      invoked.add(name);
      const entry = expected.get(name);
      if (!entry) {
        throw new Error(`Frontend calls unknown contract function: ${name}`);
      }
      return entry;
    };

    const health = record("health");
    expect(health.inputs).toEqual([]);
    expect(health.outputs).toEqual(["Health"]);

    const balance = record("balance_of");
    expect(balance.inputs.map((input) => input.type)).toEqual(["Address"]);
    expect(balance.outputs).toEqual(["u64"]);

    const latest = record("get_latest_wrap");
    expect(latest.inputs.map((input) => input.type)).toEqual(["Address"]);
    expect(latest.outputs).toEqual(["WrapRecord"]);

    const wrap = record("get_wrap");
    expect(wrap.inputs.map((input) => input.type)).toEqual(["Address", "u64"]);
    expect(wrap.outputs).toEqual(["WrapRecord"]);

    const mint = record("mint_wrap");
    expect(mint.inputs.map((input) => input.type)).toEqual([
      "Address",
      "u64",
      "Symbol",
      "Bytes",
      "Bytes",
    ]);
    expect(mint.outputs).toEqual(["void"]);

    const specNames = new Set(spec.funcs().map((func) => func.name()));
    for (const name of invoked) {
      expect(specNames.has(name)).toBe(true);
    }
    expect(invoked.size).toBe(expected.size);
  });

  it("fails when the contract spec drops a function the frontend calls", () => {
    const spec = contractSpec();
    const names = spec.funcs().map((func) => func.name());
    expect(names).toContain("mint_wrap");
    expect(names).toContain("get_wrap");
    expect(names).toContain("get_latest_wrap");
    expect(names).toContain("balance_of");
    expect(names).toContain("health");
  });

  it("fails when a contract function changes argument order or type", () => {
    const entry = specEntry("mint_wrap");
    expect(entry.inputs.map((input) => input.type)).toEqual([
      "Address",
      "u64",
      "Symbol",
      "Bytes",
      "Bytes",
    ]);
    expect(entry.inputs.map((input) => input.name)).toEqual([
      "owner",
      "period",
      "archetype",
      "data_hash",
      "signature",
    ]);
  });

  it("loads health, balance, and wraps through read-only simulations", async () => {
    mocks.server.simulateTransaction.mockImplementation(
      async (transaction: Transaction) => {
        switch (contractMethod(transaction)) {
          case "health":
            return simulationSuccess(
              nativeToScVal({
                has_admin: true,
                has_signing_key: true,
                initialized: true,
              }),
            );
          case "balance_of":
            return simulationSuccess(nativeToScVal(2n));
          case "get_latest_wrap":
            return simulationSuccess();
          case "get_all_wraps_for_user":
            return simulationSuccess(xdr.ScVal.scvVec([]));
          default:
            throw new Error("Unexpected contract method");
        }
      },
    );

    await expect(loadDashboard(CONFIG, ADDRESS)).resolves.toEqual({
      health: {
        hasAdmin: true,
        hasSigningKey: true,
        initialized: true,
      },
      balance: 2n,
      latestWrap: null,
      records: [],
    });
    expect(mocks.server.simulateTransaction).toHaveBeenCalledTimes(4);
  });

  it("decodes a period lookup result", async () => {
    const hash = new Uint8Array(32).fill(0xab);
    mocks.server.simulateTransaction.mockResolvedValue(
      simulationSuccess(
        nativeToScVal({
          archetype: "builder",
          data_hash: hash,
          period: 202607n,
          created_at: 1_700_000_000n,
        }),
      ),
    );

    await expect(getWrap(CONFIG, ADDRESS, 202607n)).resolves.toEqual({
      archetype: "builder",
      dataHash: "ab".repeat(32),
      period: 202607n,
      createdAt: 1_700_000_000n,
    });

    const transaction = mocks.server.simulateTransaction.mock
      .calls[0][0] as Transaction;
    const invocation = contractInvocation(transaction);
    expect(invocation.functionName().toString()).toBe("get_wrap");
    expect(scValToNative(invocation.args()[1])).toBe(202607n);
  });

  it("encodes mint arguments, signs the assembled XDR, and confirms submission", async () => {
    await expect(
      mintWrap(CONFIG, ADDRESS, {
        archetype: "builder",
        dataHash: new Uint8Array(32).fill(0x11),
        period: 202607n,
        signature: new Uint8Array(64).fill(0x22),
      }),
    ).resolves.toBe("transaction-hash");

    const transaction = mocks.server.simulateTransaction.mock
      .calls[0][0] as Transaction;
    const invocation = contractInvocation(transaction);
    const args = invocation.args();
    expect(invocation.functionName().toString()).toBe("mint_wrap");
    expect(args.map((arg) => arg.switch().name)).toEqual([
      "scvAddress",
      "scvU64",
      "scvSymbol",
      "scvBytes",
      "scvBytes",
    ]);
    expect(scValToNative(args[1])).toBe(202607n);
    expect(scValToNative(args[2])).toBe("builder");
    expect(Array.from(scValToNative(args[3]) as Uint8Array)).toEqual(
      Array(32).fill(0x11),
    );
    expect(Array.from(scValToNative(args[4]) as Uint8Array)).toEqual(
      Array(64).fill(0x22),
    );
    expect(signWithFreighter).toHaveBeenCalledWith(
      transaction.toXDR(),
      ADDRESS,
      PASSPHRASE,
    );
    expect(mocks.server.sendTransaction).toHaveBeenCalledOnce();
    expect(mocks.server.pollTransaction).toHaveBeenCalledWith(
      "transaction-hash",
      expect.objectContaining({ attempts: 30 }),
    );
  });

  it("does not request a wallet signature after failed simulation", async () => {
    mocks.server.simulateTransaction.mockResolvedValue({
      _parsed: true,
      error: "host function failed",
      events: [],
      id: "simulation",
      latestLedger: 1,
    });

    await expect(
      mintWrap(CONFIG, ADDRESS, {
        archetype: "builder",
        dataHash: new Uint8Array(32),
        period: 202607n,
        signature: new Uint8Array(64),
      }),
    ).rejects.toThrow("Contract simulation failed: host function failed");
    expect(signWithFreighter).not.toHaveBeenCalled();
    expect(mocks.server.sendTransaction).not.toHaveBeenCalled();
  });

  it("reports rejected submissions", async () => {
    mocks.server.sendTransaction.mockResolvedValue({
      hash: "transaction-hash",
      status: "ERROR",
    });

    await expect(
      mintWrap(CONFIG, ADDRESS, {
        archetype: "builder",
        dataHash: new Uint8Array(32),
        period: 202607n,
        signature: new Uint8Array(64),
      }),
    ).rejects.toThrow("Transaction submission failed with status ERROR");
    expect(mocks.server.pollTransaction).not.toHaveBeenCalled();
  });

  it("reports transactions that fail during confirmation", async () => {
    mocks.server.pollTransaction.mockResolvedValue({
      status: rpc.Api.GetTransactionStatus.FAILED,
    });

    await expect(
      mintWrap(CONFIG, ADDRESS, {
        archetype: "builder",
        dataHash: new Uint8Array(32),
        period: 202607n,
        signature: new Uint8Array(64),
      }),
    ).rejects.toThrow("Transaction was not confirmed: FAILED");
  });
});

describe("validateConfig", () => {
  it("trims and accepts an HTTPS configuration", () => {
    expect(
      validateConfig({
        contractId: ` ${VALID_CONTRACT} `,
        rpcUrl: " https://soroban-testnet.stellar.org ",
        networkPassphrase: " Test network ",
      }),
    ).toEqual({
      contractId: VALID_CONTRACT,
      rpcUrl: "https://soroban-testnet.stellar.org",
      networkPassphrase: "Test network",
    });
  });

  it.each([
    "http://localhost:8000",
    "http://127.0.0.1:8000",
    "http://[::1]:8000",
  ])("allows loopback HTTP for local development: %s", (rpcUrl) => {
    expect(
      validateConfig({
        contractId: VALID_CONTRACT,
        rpcUrl,
        networkPassphrase: "Standalone",
      }).rpcUrl,
    ).toBe(rpcUrl);
  });

  it("rejects malformed contract IDs", () => {
    expect(() =>
      validateConfig({
        contractId: "CINVALID",
        rpcUrl: "https://example.com",
        networkPassphrase: "Test",
      }),
    ).toThrow("valid Stellar contract ID");
  });

  it("rejects insecure remote RPC URLs", () => {
    expect(() =>
      validateConfig({
        contractId: VALID_CONTRACT,
        rpcUrl: "http://example.com",
        networkPassphrase: "Test",
      }),
    ).toThrow("must use HTTPS");
  });

  it("rejects non-HTTP protocols, including on localhost", () => {
    expect(() =>
      validateConfig({
        contractId: VALID_CONTRACT,
        rpcUrl: "ftp://localhost:8000",
        networkPassphrase: "Standalone",
      }),
    ).toThrow("must use HTTPS");
  });

  it("rejects RPC URLs containing credentials", () => {
    expect(() =>
      validateConfig({
        contractId: VALID_CONTRACT,
        rpcUrl: "https://user:password@example.com",
        networkPassphrase: "Test",
      }),
    ).toThrow("must not contain embedded credentials");
  });

  it("requires a network passphrase", () => {
    expect(() =>
      validateConfig({
        contractId: VALID_CONTRACT,
        rpcUrl: "https://example.com",
        networkPassphrase: " ",
      }),
    ).toThrow("Network passphrase is required");
  });
});
