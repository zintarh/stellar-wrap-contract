import {
  normalizeHealth,
  normalizeWrap,
  parseHexBytes,
  toHex,
  validatePeriod,
} from "./format";

describe("parseHexBytes", () => {
  it("accepts exact-length hexadecimal values with an optional prefix", () => {
    expect(parseHexBytes("0x00ff", 2, "Value")).toEqual(
      Uint8Array.from([0, 255]),
    );
  });

  it.each([
    ["xyz", "Value must contain only hexadecimal characters."],
    ["0011", "Value must be exactly 3 bytes."],
    ["", "Value must contain only hexadecimal characters."],
  ])("rejects invalid input %s", (value, message) => {
    expect(() => parseHexBytes(value, 3, "Value")).toThrow(message);
  });
});

describe("validatePeriod", () => {
  it("returns a u64-compatible bigint for a valid period", () => {
    expect(validatePeriod("202607")).toBe(202607n);
  });

  it.each(["20260", "202613", "202400", "202312", "210101"])(
    "rejects invalid period %s",
    (period) => {
      expect(() => validatePeriod(period)).toThrow();
    },
  );
});

describe("contract result normalization", () => {
  it("maps Soroban field names into the UI health model", () => {
    expect(
      normalizeHealth({
        initialized: true,
        has_admin: 1,
        has_signing_key: false,
      }),
    ).toEqual({
      initialized: true,
      hasAdmin: true,
      hasSigningKey: false,
    });
  });

  it("maps wrap records and encodes the hash", () => {
    const hash = new Uint8Array(32);
    hash.set([0, 15, 255]);

    expect(
      normalizeWrap({
        created_at: 1_700_000_000n,
        data_hash: hash,
        archetype: "builder",
        period: 202607n,
      }),
    ).toEqual({
      createdAt: 1_700_000_000n,
      dataHash: `000fff${"00".repeat(29)}`,
      archetype: "builder",
      period: 202607n,
    });
  });

  it("preserves an absent optional record", () => {
    expect(normalizeWrap(null)).toBeNull();
  });

  it.each(["not bytes", new Uint8Array(31), new Uint16Array(16)])(
    "rejects malformed record hash %s",
    (dataHash) => {
      expect(() =>
        normalizeWrap({
          created_at: 1n,
          data_hash: dataHash,
          archetype: "builder",
          period: 202607n,
        }),
      ).toThrow("invalid data hash");
    },
  );

  it("pads every byte when formatting hexadecimal output", () => {
    expect(toHex(Uint8Array.from([0, 1, 16, 255]))).toBe("000110ff");
  });
});
