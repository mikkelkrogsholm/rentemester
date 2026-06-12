import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createTar, dirToTarEntries, extractTar, readTar } from "../../src/core/tar";

import { cleanupDir } from "../helpers/cleanup";
describe("tar", () => {
  test("round-trips entries through createTar/readTar", () => {
    const entries = [
      { path: "config/policy.yaml", content: Buffer.from("company_policy:\n") },
      { path: "ledger.sqlite", content: Buffer.from([0, 1, 2, 3, 255, 128]) },
      { path: "documents-originals/faktura.pdf", content: Buffer.from("%PDF-1.4\n") },
    ];
    const archive = createTar(entries);
    expect(archive.length % 512).toBe(0);
    const read = readTar(archive);
    expect(read.map((e) => e.path)).toEqual([
      "config/policy.yaml",
      "documents-originals/faktura.pdf",
      "ledger.sqlite",
    ]);
    const ledger = read.find((e) => e.path === "ledger.sqlite")!;
    expect([...ledger.content]).toEqual([0, 1, 2, 3, 255, 128]);
  });

  test("is deterministic regardless of input order", () => {
    const a = createTar([
      { path: "b.txt", content: Buffer.from("bbb") },
      { path: "a.txt", content: Buffer.from("aaa") },
    ]);
    const b = createTar([
      { path: "a.txt", content: Buffer.from("aaa") },
      { path: "b.txt", content: Buffer.from("bbb") },
    ]);
    expect(a.equals(b)).toBe(true);
  });

  test("rejects duplicate entry paths", () => {
    expect(() =>
      createTar([
        { path: "x.txt", content: Buffer.from("1") },
        { path: "x.txt", content: Buffer.from("2") },
      ]),
    ).toThrow(/duplicate/);
  });

  test("handles a name longer than 100 bytes via the prefix field", () => {
    const longPath = `${"nested/".repeat(14)}file.txt`;
    expect(longPath.length).toBeGreaterThan(100);
    const archive = createTar([{ path: longPath, content: Buffer.from("deep") }]);
    expect(readTar(archive)[0]!.path).toBe(longPath);
  });

  test("extractTar writes files and refuses path traversal", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-tar-"));
    try {
      const archive = createTar([
        { path: "sub/one.txt", content: Buffer.from("one") },
        { path: "two.txt", content: Buffer.from("two") },
      ]);
      extractTar(archive, dir);
      expect(readFileSync(join(dir, "sub", "one.txt"), "utf8")).toBe("one");
      expect(readFileSync(join(dir, "two.txt"), "utf8")).toBe("two");

      const evil = createTar([{ path: "ok.txt", content: Buffer.from("x") }]);
      // Splice a "../escape" path into the header name field.
      evil.write("../escape".padEnd(9, "\0"), 0);
      expect(() => extractTar(evil, dir)).toThrow();
    } finally {
      cleanupDir(dir);
    }
  });

  test("detects a corrupted header via checksum", () => {
    const archive = createTar([{ path: "a.txt", content: Buffer.from("hello") }]);
    archive[5] = archive[5]! ^ 0xff; // flip a byte inside the name field
    expect(() => readTar(archive)).toThrow(/checksum/);
  });

  test("rejects an archive missing its terminating zero blocks", () => {
    const archive = createTar([{ path: "a.txt", content: Buffer.from("hello") }]);
    // Drop the two trailing zero blocks — a truncated/interrupted transfer.
    const truncated = archive.subarray(0, archive.length - 1024);
    expect(() => readTar(truncated)).toThrow(/terminating zero block/);
  });

  test("rejects an archive whose entry body is truncated", () => {
    const archive = createTar([{ path: "a.txt", content: Buffer.from("hello") }]);
    // Keep only the header block — the declared 5-byte body is gone.
    expect(() => readTar(archive.subarray(0, 512))).toThrow(/truncated/);
  });

  test("dirToTarEntries walks a directory tree into relative POSIX paths", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-tar-walk-"));
    try {
      const seed = createTar([
        { path: "config/policy.yaml", content: Buffer.from("p") },
        { path: "ledger.sqlite", content: Buffer.from("l") },
      ]);
      extractTar(seed, dir);
      const entries = dirToTarEntries(dir);
      expect(entries.map((e) => e.path).sort()).toEqual(["config/policy.yaml", "ledger.sqlite"]);
      expect(existsSync(join(dir, "config", "policy.yaml"))).toBe(true);
    } finally {
      cleanupDir(dir);
    }
  });
});
