import { describe, expect, test } from "bun:test";
import { normalizeCvr } from "../../src/core/company";

describe("normalizeCvr", () => {
  test("strips existing DK prefix before re-applying", () => {
    expect(normalizeCvr("DK12345678")).toBe("DK12345678");
    expect(normalizeCvr("dk12345678")).toBe("DK12345678");
    expect(normalizeCvr("12345678")).toBe("DK12345678");
    expect(normalizeCvr(" DK12345678 ")).toBe("DK12345678");
    expect(normalizeCvr(" 12345678 ")).toBe("DK12345678");
  });

  test("returns null for nullish / empty input", () => {
    expect(normalizeCvr(null)).toBeNull();
    expect(normalizeCvr(undefined)).toBeNull();
    expect(normalizeCvr("")).toBeNull();
    expect(normalizeCvr("   ")).toBeNull();
  });

  test("rejects invalid input", () => {
    expect(() => normalizeCvr("1234567")).toThrow();
    expect(() => normalizeCvr("123456789")).toThrow();
    expect(() => normalizeCvr("ABCDEFGH")).toThrow();
    expect(() => normalizeCvr("DK1234567")).toThrow();
    expect(() => normalizeCvr("DK1234567A")).toThrow();
  });
});
