import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  computeRuleSetDigest,
  getReleaseProvenance,
} from "../../src/core/release-provenance";
import { CURRENT_SCHEMA_VERSION } from "../../src/core/schema-version";

describe("release provenance", () => {
  test("is stable for identical regulatory inputs and changes with bytes", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-rules-digest-"));
    mkdirSync(join(root, "rules", "dk"), { recursive: true });
    mkdirSync(join(root, "sources"), { recursive: true });
    writeFileSync(join(root, "rules", "dk", "bookkeeping.yaml"), "version: 1\n");
    writeFileSync(join(root, "sources", "scope.yaml"), "scope: one\n");

    const first = computeRuleSetDigest(root);
    expect(computeRuleSetDigest(root)).toBe(first);
    writeFileSync(join(root, "sources", "scope.yaml"), "scope: two\n");
    expect(computeRuleSetDigest(root)).not.toBe(first);
    expect(first).toMatch(/^sha256:[0-9a-f]{64}$/);
    rmSync(root, { recursive: true, force: true });
  });

  test("is independent of creation order and host locale collation", () => {
    const firstRoot = mkdtempSync(join(tmpdir(), "rentemester-rules-order-a-"));
    const secondRoot = mkdtempSync(join(tmpdir(), "rentemester-rules-order-b-"));
    try {
      for (const root of [firstRoot, secondRoot]) {
        mkdirSync(join(root, "rules", "dk"), { recursive: true });
        mkdirSync(join(root, "sources"), { recursive: true });
      }
      const files: Array<[string, string]> = [
        ["rules/dk/Z.yaml", "Z\n"],
        ["rules/dk/a.yaml", "a\n"],
        ["sources/æ.yaml", "ae\n"],
      ];
      for (const [path, content] of files) {
        writeFileSync(join(firstRoot, path), content);
      }
      for (const [path, content] of files.toReversed()) {
        writeFileSync(join(secondRoot, path), content);
      }

      expect(computeRuleSetDigest(firstRoot)).toBe(computeRuleSetDigest(secondRoot));
    } finally {
      rmSync(firstRoot, { recursive: true, force: true });
      rmSync(secondRoot, { recursive: true, force: true });
    }
  });

  test("combines product, schema and rule identities", () => {
    const provenance = getReleaseProvenance();
    expect(provenance.product.version).toBe("0.3.0");
    expect(provenance.product.bunVersion).toBeNull();
    expect(provenance.product.baseImageDigest).toBeNull();
    expect(provenance.schema.version).toBe(CURRENT_SCHEMA_VERSION);
    expect(provenance.schema.baselineChecksum).toMatch(/^[0-9a-f]{64}$/);
    expect(provenance.rules.digest).toMatch(/^sha256:[0-9a-f]{64}$/);
  });
});
