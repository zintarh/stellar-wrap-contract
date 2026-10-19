import {
  getNetwork,
  isConnected,
  requestAccess,
  signTransaction,
} from "@stellar/freighter-api";
import {
  FreighterError,
  connectWallet,
  signWithFreighter,
} from "./freighter";

vi.mock("@stellar/freighter-api", () => ({
  getNetwork: vi.fn(),
  isConnected: vi.fn(),
  requestAccess: vi.fn(),
  signTransaction: vi.fn(),
}));

const ADDRESS = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const PASSPHRASE = "Test SDF Network ; September 2015";

describe("Freighter adapter", () => {
  beforeEach(() => {
    vi.mocked(isConnected).mockResolvedValue({ isConnected: true });
    vi.mocked(requestAccess).mockResolvedValue({ address: ADDRESS });
    vi.mocked(getNetwork).mockResolvedValue({
      network: "TESTNET",
      networkPassphrase: PASSPHRASE,
    });
    vi.mocked(signTransaction).mockResolvedValue({
      signedTxXdr: "signed-xdr",
      signerAddress: ADDRESS,
    });
  });

  it("requests wallet access and returns the active account and network", async () => {
    await expect(connectWallet()).resolves.toEqual({
      address: ADDRESS,
      network: "TESTNET",
      networkPassphrase: PASSPHRASE,
    });
    expect(isConnected).toHaveBeenCalledOnce();
    expect(requestAccess).toHaveBeenCalledOnce();
    expect(getNetwork).toHaveBeenCalledOnce();
  });

  it("classifies a missing extension as not-installed", async () => {
    vi.mocked(isConnected).mockResolvedValue({ isConnected: false });

    await expect(connectWallet()).rejects.toMatchObject({
      code: "not-installed",
      message: expect.stringContaining("not installed"),
    });
    expect(requestAccess).not.toHaveBeenCalled();
  });

  it("classifies a locked wallet as locked", async () => {
    vi.mocked(requestAccess).mockRejectedValue(
      new Error("Freighter is locked"),
    );

    await expect(connectWallet()).rejects.toMatchObject({
      code: "locked",
      message: expect.stringContaining("locked"),
    });
    expect(getNetwork).not.toHaveBeenCalled();
  });

  it("acknowledges a declined access request as a user choice", async () => {
    vi.mocked(requestAccess).mockResolvedValue({ address: "" });

    await expect(connectWallet()).rejects.toMatchObject({
      code: "declined",
      message: expect.stringContaining("declined"),
    });
    expect(getNetwork).not.toHaveBeenCalled();
  });

  it("reports a network mismatch with expected and actual networks", async () => {
    vi.mocked(getNetwork).mockResolvedValue({
      network: "PUBLIC",
      networkPassphrase: "Public Global Stellar Network ; September 2015",
    });

    await expect(connectWallet()).rejects.toMatchObject({
      code: "wrong-network",
      message: expect.stringContaining("TESTNET"),
    });
    await expect(connectWallet()).rejects.toMatchObject({
      message: expect.stringContaining("PUBLIC"),
    });
  });

  it("rejects an empty network response as an RPC failure", async () => {
    vi.mocked(getNetwork).mockResolvedValue({
      network: "",
      networkPassphrase: "",
    });

    await expect(connectWallet()).rejects.toMatchObject({
      code: "rpc-failure",
      message: expect.stringContaining("Could not read"),
    });
  });

  it("passes the account and passphrase when requesting a signature", async () => {
    await expect(
      signWithFreighter("unsigned-xdr", ADDRESS, PASSPHRASE),
    ).resolves.toBe("signed-xdr");
    expect(signTransaction).toHaveBeenCalledWith("unsigned-xdr", {
      address: ADDRESS,
      networkPassphrase: PASSPHRASE,
    });
  });

  it("acknowledges a declined signature without retrying", async () => {
    vi.mocked(signTransaction).mockRejectedValue(
      new Error("User declined the request"),
    );

    await expect(
      signWithFreighter("unsigned-xdr", ADDRESS, PASSPHRASE),
    ).rejects.toMatchObject({
      code: "declined",
      message: expect.stringContaining("declined"),
    });
    expect(signTransaction).toHaveBeenCalledOnce();
  });

  it("classifies a locked wallet during signing", async () => {
    vi.mocked(signTransaction).mockRejectedValue(
      new Error("Freighter is locked"),
    );

    await expect(
      signWithFreighter("unsigned-xdr", ADDRESS, PASSPHRASE),
    ).rejects.toMatchObject({ code: "locked" });
  });

  it("classifies a genuine RPC failure during signing", async () => {
    vi.mocked(signTransaction).mockRejectedValue(
      new Error("Network request failed"),
    );

    await expect(
      signWithFreighter("unsigned-xdr", ADDRESS, PASSPHRASE),
    ).rejects.toMatchObject({ code: "rpc-failure" });
  });

  it("rejects an empty signed transaction response as an RPC failure", async () => {
    vi.mocked(signTransaction).mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
    });

    await expect(
      signWithFreighter("unsigned-xdr", ADDRESS, PASSPHRASE),
    ).rejects.toMatchObject({
      code: "rpc-failure",
      message: expect.stringContaining("did not sign"),
    });
  });

  it("exposes FreighterError for callers to branch on", () => {
    const err = new FreighterError("declined", "You declined the request");
    expect(err).toBeInstanceOf(Error);
    expect(err.code).toBe("declined");
  });
});
