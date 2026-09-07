import { describe, expect, test } from "bun:test";
import { verifyProductionLicenses } from "../../scripts/release/check-production-licenses";

describe("production license gate", () => {
  test("accepts the OSI-approved zero-clause BSD license", () => {
    expect(verifyProductionLicenses({
      "0BSD": [{ name: "tslib", versions: ["2.8.1"], license: "0BSD" }],
      Unlicense: [{ name: "robust-predicates", versions: ["3.0.3"], license: "Unlicense" }],
    })).toEqual({ licenses: ["0BSD", "Unlicense"], packages: 2 });
  });

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
});
