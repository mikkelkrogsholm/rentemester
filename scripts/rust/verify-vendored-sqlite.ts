import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
const root = resolve(import.meta.dir, "../..");
const vendor = join(root, "vendor");
const manifest = JSON.parse(readFileSync(join(vendor, "sqlite-provenance.json"), "utf8"));
const digest = (p: string) => createHash("sha256").update(readFileSync(p)).digest("hex");
assert.equal(manifest.format, "rentemester-vendored-sqlite/1");
assert.deepEqual(Object.keys(manifest.packages).sort(), ["libsqlite3-sys-0.38.2", "rusqlite-0.40.2"]);
const expectedChanges: Record<string, string[]> = {
  "libsqlite3-sys-0.38.2": ["sqlite3/sqlite3.c"],
  "rusqlite-0.40.2": ["Cargo.toml", "Cargo.toml.orig", "src/lib.rs"],
};
let checked = 0;
for (const [name, row] of Object.entries(manifest.packages) as [string, any][]) {
  const dir = join(vendor, name);
  const files: string[] = [];
  const walk = (relative: string) => {
    for (const entry of readdirSync(join(dir, relative), { withFileTypes: true })) {
      const path = relative ? `${relative}/${entry.name}` : entry.name;
      assert(!entry.isSymbolicLink(), `unexpected vendored symlink: ${name}/${path}`);
      if (entry.isDirectory()) walk(path);
      else { assert(entry.isFile()); files.push(path); }
    }
  };
  walk("");
  assert.deepEqual(files.sort(), Object.keys(row.upstream_files).sort());
  assert.deepEqual(Object.keys(row.modified_files).sort(), expectedChanges[name]);
  for (const file of files) {
    assert.equal(digest(join(dir, file)), row.modified_files[file] ?? row.upstream_files[file], `${name}/${file}`);
    checked++;
  }
}
assert.equal(digest(join(vendor, "sqlite-identity.patch")), manifest.patch_sha256);
console.log(JSON.stringify({ok: true, scope: "exact upstream snapshot and explicit identity-only patch inventory", packages: 2, files: checked, modifiedFiles: 4, deploymentQualification: false}));
