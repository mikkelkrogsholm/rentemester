import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
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
      ? ["num-bigint", "serde"]
      : ["serde", "rentemester-engine", "rentemester-storage", "rusqlite"],
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
for (const crate of ["rentemester-engine", "rentemester-application"]) {
  if (crate === "rentemester-engine")
    assert.throws(() => checkDependencies(crate, ["serde", "rusqlite"]));
  assert.throws(() => checkDependencies(crate, ["serde", "rustix"]));
}
assert.throws(() => checkSource("use std::fs::File;"));
assert.throws(() => checkSource("use rusqlite::Connection;"));
assert.throws(() => checkSource("rentemester_storage::control_access::observe_human();"));
console.log(
  JSON.stringify({
    ok: true,
    scope:
      "current domain/application dependency boundaries and pure domain/access module imports",
    negativeControls: [
      "forbidden domain SQL and unclassified OS dependencies",
      "direct filesystem/SQL/storage imports in pure modules",
    ],
    limits:
      "Direct import scan supplements dependency inspection and code review; it is not an exhaustive Rust semantic analyzer.",
  }),
);
