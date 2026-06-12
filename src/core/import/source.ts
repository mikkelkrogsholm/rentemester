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
import { mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
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
 * Strips ASCII/C0 control bytes (incl. ANSI escape, NUL, CR/LF) and C1 control
 * bytes from a string destined for an Error message. A malicious zip filename
 * or a hostile unzip stderr line can otherwise inject terminal escape
 * sequences (clear-screen, fake hyperlinks via OSC, ...) into CLI output that
 * gets echoed verbatim by the default Node uncaughtException printer. We
 * collapse the stripped runs into single spaces so the message stays readable
 * and single-line.
 */
function sanitizeForErrorMessage(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\u0000-\u001f\u007f-\u009f]/g, " ").replace(/\s+/g, " ").trim();
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
  // Sanitize the user-controlled zipPath once: every error message below
  // interpolates the safe variant so a malicious filename cannot inject
  // ANSI/OSC terminal escapes (or newlines) into CLI output.
  const safeZipPath = sanitizeForErrorMessage(zipPath);
  try {
    // #199 — Dinero-exports carry some entry names i CP437 (legacy DOS-encoding),
    // især `Ikke-bogførte-bilag/`-mappen. På Info-ZIP-builds (Linux) tager
    // `-O CP437` flaget den encoding og transcoder til filesystem-locale — så
    // de danske tegn overlever som UTF-8. På BSD unzip / Apple's Info-ZIP build
    // honorerer flaget ikke (empirisk: exit 10 med usage-banner, ingen
    // udpakning); vi falder tilbage til plain unzip og tolerer at de få entries
    // med ikke-UTF-8 navne droppes (#192's eksisterende mitigation).
    //
    // Detektering: hvis CP437-forsøget producerede et tomt dest, kører
    // fallback'en. Hvis det producerede indhold, beholder vi det (selv om
    // unzip ekstrahere "uden transcoding", er det funktionelt det samme som
    // fallback'en ville give — plus eventuelt korrekt-transcodede navne på
    // platforme hvor -O virker).
    const tryWithCharset = spawnSync(
      "unzip",
      ["-q", "-O", "CP437", "-o", zipPath, "-d", dest],
      { encoding: "utf8" },
    );
    const cp437Stderr = (tryWithCharset.stderr ?? "").trim();
    const charsetExtractedSomething =
      !tryWithCharset.error && readdirSync(dest).length > 0;
    let result = tryWithCharset;
    if (!charsetExtractedSomething) {
      // CP437-forsøget gav et tomt dest — vi kører fallback'en. Tøm dest IN-PLACE
      // (ikke rm-and-mkdir) for at bevare mkdtempSync's 0o700-mode og undgå
      // TOCTOU-vindue på shared /tmp. I praksis er dest tomt her (verificeret
      // empirisk på macOS), men vi rydder defensivt for at undgå at lade et
      // halvt-pakket træ fra et fremtidigt unzip-build forurene fallback'en.
      // ENOENT (dest racing with an external tmp cleaner) is swallowed to
      // match the old `rmSync(force:true)` semantics — the fallback spawn
      // below will fail clearly if dest is truly gone.
      try {
        for (const entry of readdirSync(dest)) {
          rmSync(join(dest, entry), { recursive: true, force: true });
        }
      } catch (err) {
        const code = (err as NodeJS.ErrnoException | undefined)?.code;
        if (code !== "ENOENT") {
          const detail = sanitizeForErrorMessage(
            err instanceof Error ? err.message : String(err),
          );
          throw new Error(
            `failed to prepare fallback directory for '${safeZipPath}': ${detail}`,
          );
        }
      }
      result = spawnSync("unzip", ["-q", "-o", zipPath, "-d", dest], { encoding: "utf8" });
    }
    if (result.error) {
      throw new Error(
        `failed to run 'unzip' for '${safeZipPath}': ${sanitizeForErrorMessage(result.error.message)}`,
      );
    }
    if (readdirSync(dest).length === 0) {
      const fallbackDetailRaw = (result.stderr || "").trim().split(/\r?\n/)[0] ?? "";
      // Begge forsøg fejlede — surface både CP437- og fallback-stderr så
      // operatøren kan diagnosticere root cause uden at miste den ene.
      const cp437DetailRaw = cp437Stderr ? (cp437Stderr.split(/\r?\n/)[0] ?? "") : "";
      const fallbackDetail = sanitizeForErrorMessage(fallbackDetailRaw);
      const cp437Detail = sanitizeForErrorMessage(cp437DetailRaw);
      const detail = [
        fallbackDetail,
        cp437Detail && cp437Detail !== fallbackDetail ? `cp437 attempt: ${cp437Detail}` : "",
      ]
        .filter(Boolean)
        .join("; ");
      throw new Error(
        `unzip extracted nothing from '${safeZipPath}'` +
          (typeof result.status === "number" ? ` (exit ${result.status})` : "") +
          (detail ? `: ${detail}` : ""),
      );
    }
    return dest;
  } catch (err) {
    // Clean up the mkdtempSync directory on every failure path so a long-
    // running cockpit/bilagsmail server doesn't leak `/tmp/rentemester-import-*`
    // trees full of partially-extracted Dinero receipts across days. The
    // success path returns above without entering this catch.
    try {
      rmSync(dest, { recursive: true, force: true });
    } catch {
      // Swallow cleanup failures — we're already throwing the primary error.
    }
    throw err;
  }
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
