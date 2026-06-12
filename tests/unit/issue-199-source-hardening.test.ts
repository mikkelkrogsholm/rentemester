// Follow-up adversarial review of commit cdf2cbe — hardening of source.ts's
// unzipToTempDir beyond the initial #199 fix:
//
//   1. Strict permission preservation under umask=0o022 (catches the original
//      regression on every platform regardless of which fallback branch runs).
//   2. Fallback path is exercised on every platform via a PATH-stub `unzip`
//      that rejects -O CP437 — guarantees the wipe-loop code runs even on
//      Linux/Info-ZIP where the production CP437 attempt succeeds.
//   3. Temp directory is cleaned up after both attempts fail (so a long-
//      running cockpit doesn't accumulate /tmp/rentemester-import-* dirs of
//      private receipt PDFs).
//   4. Error messages strip control characters from user-controlled `zipPath`
//      and unzip stderr — defends against ANSI/OSC injection into CLI output.

import { describe, expect, test } from "bun:test";
import {
  mkdtempSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { resolveSource } from "../../src/core/import/source";

function zipDir(srcDir: string): string {
  const zipPath = join(mkdtempSync(join(tmpdir(), "rm-harden-zip-")), "export.zip");
  const result = spawnSync("zip", ["-q", "-r", zipPath, "."], { cwd: srcDir });
  expect(result.status).toBe(0);
  return zipPath;
}

/**
 * Builds a stub `unzip` binary on PATH that rejects `-O CP437` (exit 10 + the
 * macOS-style usage banner) and otherwise execs the system unzip. Returns a
 * function that restores the original PATH.
 *
 * This forces the fallback branch of unzipToTempDir to run regardless of
 * whether the host's real unzip implements `-O CP437` — closing the test gap
 * caught in adversarial review where Linux CI runners would silently miss
 * regressions.
 */
function withUnzipStubRejectingO(): { restore: () => void } {
  const stubDir = mkdtempSync(join(tmpdir(), "rm-unzip-stub-"));
  const which = spawnSync("which", ["unzip"], { encoding: "utf8" });
  const realUnzip = (which.stdout ?? "").trim() || "/usr/bin/unzip";
  writeFileSync(
    join(stubDir, "unzip"),
    [
      "#!/bin/sh",
      // If -O appears anywhere in argv, pretend we don't understand it.
      'for a in "$@"; do',
      '  if [ "$a" = "-O" ]; then',
      '    echo "UnZip stub: caution: -O is for DOS, OS/2, NT only" >&2',
      "    exit 10",
      "  fi",
      "done",
      `exec "${realUnzip}" "$@"`,
    ].join("\n"),
    { mode: 0o755 },
  );
  const originalPath = process.env.PATH;
  process.env.PATH = `${stubDir}:${originalPath ?? ""}`;
  return {
    restore: () => {
      if (originalPath === undefined) delete process.env.PATH;
      else process.env.PATH = originalPath;
      rmSync(stubDir, { recursive: true, force: true });
    },
  };
}

describe("#199 hardening — strict permission preservation", () => {
  test("dest mode is EXACTLY 0o700 under umask 0o022 (default Linux/macOS)", () => {
    const previousUmask = process.umask(0o022);
    const src = mkdtempSync(join(tmpdir(), "rm-harden-src-"));
    mkdirSync(join(src, "2025"));
    writeFileSync(join(src, "Firmaoplysninger.csv"), "Firmanavn\nTest ApS\n");
    writeFileSync(join(src, "2025", "Kontoplan.csv"), "Nummer;Navn\n1000;Salg\n");
    const zipPath = zipDir(src);
    try {
      const resolved = resolveSource(zipPath);
      const mode = statSync(resolved.rootDir).mode & 0o777;
      // Exact match (not just `mode & 0o077 === 0`): rules out the buggy
      // mkdirSync-with-umask=0o077 tautology and pins the EXACT mkdtempSync mode.
      expect(mode).toBe(0o700);
    } finally {
      process.umask(previousUmask);
      rmSync(src, { recursive: true, force: true });
      rmSync(zipPath, { recursive: true, force: true });
    }
  });
});

describe("#199 hardening — fallback path is exercised on every platform", () => {
  test("dest mode is 0o700 even when the CP437 attempt is rejected (fallback runs)", () => {
    const previousUmask = process.umask(0o022);
    const stub = withUnzipStubRejectingO();
    const src = mkdtempSync(join(tmpdir(), "rm-harden-fbk-src-"));
    mkdirSync(join(src, "2025"));
    writeFileSync(join(src, "Firmaoplysninger.csv"), "Firmanavn\nTest ApS\n");
    writeFileSync(join(src, "2025", "Kontoplan.csv"), "Nummer;Navn\n1000;Salg\n");
    const zipPath = zipDir(src);
    try {
      const resolved = resolveSource(zipPath);
      // Extraction must still succeed (fallback runs plain unzip).
      expect(resolved.files["Firmaoplysninger.csv"]).toBeDefined();
      expect(resolved.files["2025/Kontoplan.csv"]).toBeDefined();
      // And the permission must still be exact 0o700 — i.e. the wipe loop did
      // NOT regress to the rmSync+mkdirSync pattern that dropped the mode.
      const mode = statSync(resolved.rootDir).mode & 0o777;
      expect(mode).toBe(0o700);
    } finally {
      stub.restore();
      process.umask(previousUmask);
      rmSync(src, { recursive: true, force: true });
      rmSync(zipPath, { recursive: true, force: true });
    }
  });
});

describe("#199 hardening — temp dir cleanup on failure", () => {
  test("a malformed .zip is cleaned up: no rentemester-import-* dir is left behind", () => {
    const previousUmask = process.umask(0o022);
    // A non-empty, non-zip blob with a .zip extension. Both attempts fail.
    const badZip = join(mkdtempSync(join(tmpdir(), "rm-harden-bad-")), "broken.zip");
    writeFileSync(badZip, "not a zip at all\n");
    const tmpRootSnapshot = readdirSync(tmpdir()).filter((n) =>
      n.startsWith("rentemester-import-"),
    );
    try {
      expect(() => resolveSource(badZip)).toThrow(/unzip/i);
      const after = readdirSync(tmpdir()).filter((n) =>
        n.startsWith("rentemester-import-"),
      );
      // The function must not leak its mkdtempSync directory on failure.
      // (The set MUST NOT have grown.)
      expect(after.length).toBeLessThanOrEqual(tmpRootSnapshot.length);
    } finally {
      process.umask(previousUmask);
      rmSync(badZip, { recursive: true, force: true });
    }
  });
});

describe("#199 hardening — control-character sanitization in error messages", () => {
  test("an attacker-controlled zipPath cannot inject ANSI/OSC escapes into the thrown error", () => {
    // A filename containing a CSI escape (clear screen) and a newline.
    const sneaky = join(
      mkdtempSync(join(tmpdir(), "rm-harden-ansi-")),
      "evil\x1b[2J\nname.zip",
    );
    writeFileSync(sneaky, "not a zip");
    try {
      let caught: unknown = null;
      try {
        resolveSource(sneaky);
      } catch (err) {
        caught = err;
      }
      const msg = caught instanceof Error ? caught.message : String(caught ?? "");
      // The escape byte and the raw newline must NOT appear in the message.
      expect(msg).not.toContain("\x1b");
      expect(msg.includes("\n")).toBe(false);
    } finally {
      rmSync(sneaky, { recursive: true, force: true });
    }
  });
});
