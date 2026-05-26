// Import framework — multi-file export resolution. Issue #192.
//
// A real accounting-system export is not a single file. A Dinero export is a
// directory tree (`Firmaoplysninger.csv` and a per-fiscal-year `Kontoplan.csv`,
// `Posteringer.csv`, ...). `resolveSource` walks an export directory into a
// `MultiArtifactSource` — every file keyed by its export-root-relative,
// forward-slash name, with a BOM-stripped UTF-8 text decode and the raw bytes.
//
// The resolution is DETERMINISTIC: directory entries are read in sorted order
// so the resulting `files` map and any derived ordering is reproducible.

import { spawnSync } from "node:child_process";
import { mkdtempSync, readdirSync, readFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ImportArtifact, MultiArtifactSource } from "./types";

/** Strips a leading UTF-8 BOM (U+FEFF) from a decoded string. */
function stripBom(text: string): string {
  return text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;
}

/**
 * Recursively collects every file under `dir`, keyed by its path relative to
 * `rootDir` using forward slashes. Directory entries are visited in sorted
 * order so the walk is deterministic regardless of filesystem ordering.
 */
function collect(rootDir: string, dir: string, into: Record<string, ImportArtifact>): void {
  const entries = readdirSync(dir).sort();
  for (const entry of entries) {
    const abs = join(dir, entry);
    const stat = statSync(abs);
    if (stat.isDirectory()) {
      collect(rootDir, abs, into);
      continue;
    }
    if (!stat.isFile()) continue;
    const rel = abs.slice(rootDir.length).replace(/^[/\\]/, "").split(/[/\\]/).join("/");
    const bytes = new Uint8Array(readFileSync(abs));
    into[rel] = {
      name: rel,
      path: abs,
      bytes,
      text: stripBom(new TextDecoder("utf-8").decode(bytes)),
    };
  }
}

/** True when `path` names a `.zip` file (case-insensitive). */
function isZipPath(path: string): boolean {
  return path.toLowerCase().endsWith(".zip");
}

/**
 * Extracts a `.zip` export into a fresh temporary directory and returns it.
 *
 * Uses the system `unzip` (dependency-free, present on macOS and Linux). A real
 * export can carry entry names in a non-UTF-8 encoding — e.g. a Dinero export's
 * `Ikke-bogførte-bilag/` folder — which cannot be created on a UTF-8
 * filesystem; `unzip` then exits non-zero having still extracted every other
 * entry. A non-zero exit is therefore TOLERATED as long as something was
 * extracted: the per-system parser fails clearly later (via `requireFile`) if a
 * file it actually requires was among the few that were skipped. Only a
 * completely empty extraction — or `unzip` not running at all — is a hard error.
 */
function unzipToTempDir(zipPath: string): string {
  const dest = mkdtempSync(join(tmpdir(), "rentemester-import-"));
  // #199 — Dinero-exports carry some entry names i CP437 (legacy DOS-encoding),
  // især `Ikke-bogførte-bilag/`-mappen. På Info-ZIP-builds (Linux) tager
  // `-O CP437` flaget den encoding og transcoder til filesystem-locale — så
  // de danske tegn overlever som UTF-8. På BSD unzip (macOS) eksisterer
  // flaget ikke; vi falder tilbage til plain unzip og tolerer at de få
  // entries med ikke-UTF-8 navne droppes (#192's eksisterende mitigation).
  const tryWithCharset = spawnSync(
    "unzip",
    ["-q", "-O", "CP437", "-o", zipPath, "-d", dest],
    { encoding: "utf8" },
  );
  const charsetSupported =
    !tryWithCharset.error &&
    readdirSync(dest).length > 0 &&
    !/invalid option|unknown option/i.test(tryWithCharset.stderr ?? "");
  let result = tryWithCharset;
  if (!charsetSupported) {
    // BSD unzip rejects -O — reset dest så et halvt-pakket træ fra forsøget
    // ikke smelter ind i fallback'en, og kør så plain unzip.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const fs = require("node:fs") as typeof import("node:fs");
    fs.rmSync(dest, { recursive: true, force: true });
    fs.mkdirSync(dest, { recursive: true });
    result = spawnSync("unzip", ["-q", "-o", zipPath, "-d", dest], { encoding: "utf8" });
  }
  if (result.error) {
    throw new Error(`failed to run 'unzip' for '${zipPath}': ${result.error.message}`);
  }
  if (readdirSync(dest).length === 0) {
    const detail = (result.stderr || "").trim().split(/\r?\n/)[0] ?? "";
    throw new Error(
      `unzip extracted nothing from '${zipPath}'` +
        (typeof result.status === "number" ? ` (exit ${result.status})` : "") +
        (detail ? `: ${detail}` : ""),
    );
  }
  return dest;
}

/**
 * Resolves an export `path` into a `MultiArtifactSource`. `path` may point at
 * an export directory, a `.zip` of one (extracted to a temp directory), or a
 * single file (wrapped as a one-artifact source so a single-file format still
 * works through the multi-file seam).
 *
 * Throws if `path` does not exist.
 */
export function resolveSource(path: string): MultiArtifactSource {
  const stat = statSync(path);
  if (stat.isDirectory()) {
    const files: Record<string, ImportArtifact> = {};
    collect(path, path, files);
    return { rootDir: path, files };
  }
  if (stat.isFile() && isZipPath(path)) {
    const rootDir = unzipToTempDir(path);
    const files: Record<string, ImportArtifact> = {};
    collect(rootDir, rootDir, files);
    return { rootDir, files };
  }
  // A single file: expose it under its basename.
  const name = path.split(/[/\\]/).pop() ?? path;
  const bytes = new Uint8Array(readFileSync(path));
  return {
    rootDir: path.slice(0, path.length - name.length).replace(/[/\\]$/, "") || ".",
    files: {
      [name]: {
        name,
        path,
        bytes,
        text: stripBom(new TextDecoder("utf-8").decode(bytes)),
      },
    },
  };
}

/**
 * Looks up a required file in a `MultiArtifactSource`. On a miss it appends a
 * clear, named error to `errors` and returns `null`, so a parser can collect
 * every missing file before failing.
 */
export function requireFile(
  input: MultiArtifactSource,
  name: string,
  errors: string[],
): ImportArtifact | null {
  const file = input.files[name];
  if (!file) {
    errors.push(`required export file '${name}' is missing`);
    return null;
  }
  return file;
}
