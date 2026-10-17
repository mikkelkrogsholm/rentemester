import assert from "node:assert/strict";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
for (const command of [
  [cargo, "fmt", "--all", "--", "--check"],
  [
    cargo,
    "clippy",
    "--locked",
    "--workspace",
    "--all-targets",
    "--",
    "-D",
    "warnings",
  ],
  [cargo, "test", "--locked", "--workspace"],
  [process.execPath, "run", "scripts/rust/schema-contract.ts"],
  [process.execPath, "run", "scripts/rust/check-boundaries.ts"],
  [process.execPath, "run", "scripts/rust/verify-readonly.ts"],
]) {
  const result = Bun.spawnSync(command, {
    cwd: root,
    stdout: "inherit",
    stderr: "inherit",
  });
  assert.equal(
    result.exitCode,
    0,
    `Rust verification failed: ${command.slice(1).join(" ")}`,
  );
}
