import { describe, expect, test } from "bun:test";
import { verifyProductionLicenses } from "../../scripts/release/check-production-licenses";

describe("production license gate", () => {
  test("accepts the explicit permissive allowlist", () => {
    expect(verifyProductionLicenses({
      MIT: [{ name: "example", versions: ["1.0.0"], license: "MIT" }],
      "Apache-2.0": [{ name: "example-two", versions: ["2.0.0"], license: "Apache-2.0" }],
    })).toEqual({ licenses: ["Apache-2.0", "MIT"], packages: 2 });
  });

  test("fails closed for unknown, missing or malformed license metadata", () => {
    expect(() => verifyProductionLicenses({
      GPL: [{ name: "copyleft", versions: ["1.0.0"], license: "GPL" }],
    })).toThrow("unapproved license");
    expect(() => verifyProductionLicenses({})).toThrow("contains no packages");
    expect(() => verifyProductionLicenses({
      MIT: [{ name: "missing-version", versions: [], license: "MIT" }],
    })).toThrow("invalid package metadata");
  });
  test("normalizes only the reviewed css-mediaquery release's BSD metadata", () => {
    expect(verifyProductionLicenses({ BSD: [{ name: "css-mediaquery", versions: ["0.1.2"], license: "BSD" }] }))
      .toEqual({ licenses: ["BSD-3-Clause"], packages: 1 });
    for (const entry of [
      { name: "different-package", versions: ["0.1.2"], license: "BSD" },
      { name: "css-mediaquery", versions: ["0.1.3"], license: "BSD" },
      { name: "css-mediaquery", versions: ["0.1.2", "0.1.3"], license: "BSD" },
    ]) expect(() => verifyProductionLicenses({ BSD: [entry] })).toThrow("unapproved license");
  });
});
