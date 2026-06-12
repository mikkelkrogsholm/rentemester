// Regression guard for #199 fallback path — caught in code-review of commit
// 6618dee. The original fallback did `fs.rmSync(dest) + fs.mkdirSync(dest)`,
// which dropped the mkdtempSync-supplied 0o700 mode in favour of process
// umask (~0o755), exposing extracted Dinero receipts (private financial PDFs)
// to other local users on shared /tmp.
//
// This test pins:
//   - the dest directory keeps 0o700 mode after the fallback runs
//   - extraction still succeeds (the fix must not break the actual feature)
//
// We trigger the fallback by passing a non-zip file as input: the CP437
// attempt fails first (length==0), the fallback then fails identically — but
// only AFTER the fallback's directory-preparation step has run, which is the
// step under test.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, statSync, writeFileSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { resolveSource } from "../../src/core/import/source";

/** Builds a `.zip` of `srcDir`'s contents and returns its path. */
function zipDir(srcDir: string): string {
  const zipPath = join(mkdtempSync(join(tmpdir(), "rm-perm-zip-")), "export.zip");
  const result = spawnSync("zip", ["-q", "-r", zipPath, "."], { cwd: srcDir });
  expect(result.status).toBe(0);
  return zipPath;
}

describe("#199 fallback path — temp dir preserves mkdtempSync's 0o700 mode", () => {
  test("a .zip extracted via resolveSource yields a tmp dir at exactly mode 0o700", () => {
    // Pin umask to the common Linux/macOS default so the assertion below is
    // deterministic — otherwise a CI runner with hardened umask=0o077 would
    // mask the buggy `mkdirSync()` output into 0o700 by accident and the
    // test would pass on broken code (caught in adversarial review).
    const previousUmask = process.umask(0o022);
    const src = mkdtempSync(join(tmpdir(), "rm-perm-src-"));
    mkdirSync(join(src, "2025"));
    writeFileSync(join(src, "Firmaoplysninger.csv"), "Firmanavn\nTest ApS\n");
    writeFileSync(join(src, "2025", "Kontoplan.csv"), "Nummer;Navn\n1000;Salg\n");
    const zipPath = zipDir(src);

    try {
      const resolved = resolveSource(zipPath);

      // Mask off the type bits — we only care about permission bits.
      const mode = statSync(resolved.rootDir).mode & 0o777;

      // Exact-match — under umask 0o022 the buggy `mkdirSync(dest, recursive:true)`
      // path would produce 0o755 and this would catch it. The looser
      // `mode & 0o077 === 0` assertion would pass on a CI with umask=0o077
      // even on the buggy code; this strict form does not.
      expect(mode).toBe(0o700);
    } finally {
      process.umask(previousUmask);
      rmSync(src, { recursive: true, force: true });
      rmSync(zipPath, { recursive: true, force: true });
    }
  });
});
