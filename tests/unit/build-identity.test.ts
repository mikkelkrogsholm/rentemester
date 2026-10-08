import { describe, expect, test } from "bun:test";
import { getBuildIdentity, PRODUCT_VERSION } from "../../src/core/build-identity";

describe("build identity", () => {
  test("uses the package SemVer without local-only metadata", () => {
    expect(PRODUCT_VERSION).toBe("0.3.0");
    expect(getBuildIdentity({})).toEqual({
      version: "0.3.0",
      gitCommit: null,
      builtAt: null,
      bunVersion: null,
      baseImageDigest: null,
    });
  });

  test("accepts release build metadata", () => {
    expect(
      getBuildIdentity({
        RENTEMESTER_VERSION: "0.3.0",
        RENTEMESTER_GIT_COMMIT: "81acc2f",
        RENTEMESTER_BUILT_AT: "2026-07-19T12:00:00.000Z",
        RENTEMESTER_BUN_VERSION: "1.4.0",
        RENTEMESTER_BASE_IMAGE_DIGEST: `sha256:${"b".repeat(64)}`,
      }),
    ).toEqual({
      version: "0.3.0",
      gitCommit: "81acc2f",
      builtAt: "2026-07-19T12:00:00.000Z",
      bunVersion: "1.4.0",
      baseImageDigest: `sha256:${"b".repeat(64)}`,
    });
  });

  test("rejects ambiguous build metadata", () => {
    expect(() =>
      getBuildIdentity({ RENTEMESTER_GIT_COMMIT: "main" }),
    ).toThrow("hexadecimal commit id");
    expect(() =>
      getBuildIdentity({ RENTEMESTER_BUILT_AT: "not-a-date" }),
    ).toThrow("ISO-8601 timestamp");
    expect(() =>
      getBuildIdentity({ RENTEMESTER_VERSION: "0.4.0" }),
    ).toThrow("does not match packaged version");
    expect(() => getBuildIdentity({ RENTEMESTER_BUN_VERSION: "not-semver" })).toThrow("SemVer");
    expect(() => getBuildIdentity({ RENTEMESTER_BASE_IMAGE_DIGEST: "image" })).toThrow("sha256 digest");
  });
});
