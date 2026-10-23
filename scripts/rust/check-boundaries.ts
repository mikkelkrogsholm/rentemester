import assert from "node:assert/strict";
import { readFileSync, readdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const result = Bun.spawnSync(
  [cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"],
  { cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(result.exitCode, 0, result.stderr.toString());
const metadata = JSON.parse(result.stdout.toString());
function checkDependencies(crate: string, names: string[]) {
  const allowed = new Set(
    crate === "rentemester-engine"
      ? ["num-bigint", "serde", "serde_json", "sha2", "ryu-js", "time",
          "base64", "zip", "mail-parser", "unicode-properties", "unicode-normalization"]
      : ["serde", "serde_json", "rentemester-engine", "rentemester-storage", "rentemester-runtime", "rusqlite"],
  );
  assert(
    names.every((n) => allowed.has(n)),
    `${crate} crosses pure policy boundary`,
  );
}
function checkSource(source: string) {
  // Supplemental direct-use check; dependency graph + code review remain necessary.
  assert(!/\b(rusqlite|rentemester_storage)\s*::/.test(source), "Pure policy imports SQL/storage");
  assert(
    !/\bstd\s*::\s*(fs|net|process|env|thread)\b/.test(source),
    "Domain imports an I/O/runtime API",
  );
  assert(
    !/\bstd\s*::\s*\{[^}]*\b(fs|net|process|env|thread)\b/.test(source),
    "Domain imports a grouped I/O/runtime API",
  );
}
for (const crate of ["rentemester-engine", "rentemester-application"]) {
  const pkg = metadata.packages.find((p: any) => p.name === crate);
  checkDependencies(
    crate,
    pkg.dependencies
      .filter((d: any) => d.kind !== "dev")
      .map((d: any) => d.name),
  );
  const dir = join(root, `crates/${crate}/src`);
  for (const f of readdirSync(dir).filter((f) => f.endsWith(".rs")))
    if (crate === "rentemester-engine" || f === "access.rs")
      checkSource(readFileSync(join(dir, f), "utf8"));
}
// Accepted engine codecs operate on supplied bytes; JSON/hashing/calendar values
// grant no SQL, filesystem or network authority. Keep explicit dependency admission.
assert.throws(() => checkDependencies("rentemester-engine", ["reqwest"]));
for (const crate of ["rentemester-engine", "rentemester-application"]) {
  if (crate === "rentemester-engine")
    assert.throws(() => checkDependencies(crate, ["serde", "rusqlite"]));
  assert.throws(() => checkDependencies(crate, ["serde", "rustix"]));
}
assert.throws(() => checkSource("use std::fs::File;"));
assert.throws(() => checkSource("use rusqlite::Connection;"));
assert.throws(() => checkSource("rentemester_storage::control_access::observe_human();"));
// The network SDK and MCP must never bypass the shared server with local storage.
// Legacy CLI advisory/rehearsal commands intentionally remain outside this scope.
const networkClients = ["rentemester-client", "rentemester-mcp"];
const forbiddenClientDependencies = new Set([
  "rentemester-storage", "rusqlite", "rentemester-application", "rentemester-host",
]);
function checkClientDependencies(crate: string, names: string[]) {
  assert(!names.some(name => forbiddenClientDependencies.has(name)),
    `${crate} bypasses the shared server with a local backend dependency`);
}
function checkClientSources(directory: string) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) checkClientSources(path);
    else if (entry.name.endsWith(".rs")) {
      const source = readFileSync(path, "utf8");
      assert(!/\b(?:rentemester_storage|rusqlite|rentemester_application|rentemester_host)\s*::/.test(source)
        && !/\bextern\s+crate\s+(?:rentemester_storage|rusqlite|rentemester_application|rentemester_host)\b/.test(source),
        `Client source imports a local backend: ${path}`);
    }
  }
}
for (const crate of networkClients) {
  const pkg = metadata.packages.find((p: any) => p.name === crate);
  assert(pkg, `Missing network client package: ${crate}`);
  checkClientDependencies(crate, pkg.dependencies.map((d: any) => d.name));
  checkClientSources(join(root, "crates", crate, "src"));
  assert.throws(() => checkClientDependencies(crate, ["rentemester-storage"]), /local backend dependency/);
}
// Read a real Rust file through the same recursive scanner, not graph labels.
const negative = mkdtempSync(join(tmpdir(), "rentemester-client-boundary-"));
try {
  const injected = join(negative, "forbidden.rs");
  for (const source of ["use rentemester_storage::CompanyDatabase;", "extern crate rentemester_storage as hidden;"]) {
    writeFileSync(injected, source);
    assert.throws(() => checkClientSources(negative), /Client source imports a local backend/);
  }
} finally {
  rmSync(negative, { recursive: true, force: true });
}
console.log(
  JSON.stringify({
    ok: true,
    scope:
      "domain/application boundaries plus actual SDK/MCP manifests and recursive Rust source imports",
    negativeControls: [
      "forbidden domain SQL and unclassified OS dependencies",
      "direct filesystem/SQL/storage imports in pure modules",
      "on-disk client-to-storage import and extern-crate alias refused by the actual scanner",
    ],
    limits:
      "Direct import scan supplements dependency inspection and review; it is not a Rust semantic analyzer. Legacy CLI local advisory/rehearsal commands and Tauri host lifecycle are not claimed as network-only SDK/MCP.",
  }),
);
