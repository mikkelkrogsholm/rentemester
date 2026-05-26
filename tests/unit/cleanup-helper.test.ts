import { describe, test, expect } from "bun:test";
import { mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { cleanupTempDir } from "../helpers/cleanup";

describe("cleanupTempDir (#323)", () => {
  test("removes a directory tree on first attempt (Unix happy path)", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-cleanup-test-"));
    writeFileSync(join(dir, "test.txt"), "hello");
    expect(existsSync(dir)).toBe(true);

    cleanupTempDir(dir);

    expect(existsSync(dir)).toBe(false);
  });

  test("is idempotent — does not throw when dir already gone", () => {
    const dir = join(tmpdir(), `rentemester-cleanup-gone-${Date.now()}`);
    // dir never created — force: true means rmSync won't throw,
    // and neither should our helper
    expect(() => cleanupTempDir(dir)).not.toThrow();
  });

  test("removes nested directories and files", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-cleanup-nested-"));
    const sub = join(dir, "sub", "deep");
    const { mkdirSync } = require("node:fs");
    mkdirSync(sub, { recursive: true });
    writeFileSync(join(sub, "data.db"), "fake-sqlite");
    writeFileSync(join(dir, "root.txt"), "root");

    cleanupTempDir(dir);

    expect(existsSync(dir)).toBe(false);
  });
});
