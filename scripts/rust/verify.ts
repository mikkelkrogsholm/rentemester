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
  [process.execPath, "run", "scripts/rust/company-schema-contract.ts"],
  [process.execPath, "run", "scripts/rust/company-physical-contract.ts"],
  [process.execPath, "run", "scripts/rust/check-boundaries.ts"],
  [process.execPath, "run", "scripts/rust/manual-rule-contract.ts"],
  [process.execPath, "run", "scripts/rust/verify-access-policy.ts"],
  [process.execPath, "run", "scripts/rust/verify-control-fence.ts"],
  [process.execPath, "run", "scripts/rust/verify-session-cookie.ts"],
  [process.execPath, "run", "scripts/rust/verify-password-crypto.ts"],
  [process.execPath, "run", "scripts/rust/verify-password-login.ts"],
  [process.execPath, "run", "scripts/rust/verify-mfa-crypto.ts"],
  [process.execPath, "run", "scripts/rust/verify-mfa-login.ts"],
  [process.execPath, "run", "scripts/rust/verify-auth-mail.ts"],
  [process.execPath, "run", "scripts/rust/verify-http-reset.ts"],
  [process.execPath, "run", "scripts/rust/verify-http-recovery.ts"],
  [process.execPath, "run", "scripts/rust/verify-http-login.ts"],
  [process.execPath, "run", "scripts/rust/verify-email-token.ts"],
  [process.execPath, "run", "scripts/rust/verify-readonly.ts"],
  [process.execPath, "run", "scripts/rust/verify-report-contracts.ts"],
  [process.execPath, "run", "scripts/rust/verify-journal-hashes.ts"],
  [process.execPath, "run", "scripts/rust/verify-workspace-lease.ts"],
  [process.execPath, "run", "scripts/rust/verify-workspace-layout.ts"],
  [process.execPath, "run", "scripts/rust/verify-migration-rehearsal.ts"],
  [process.execPath, "run", "scripts/rust/verify-control-migrations.ts"],
  [process.execPath, "run", "scripts/rust/verify-company-baseline.ts"],
  [process.execPath, "run", "scripts/rust/verify-company-normalization.ts"],
  [process.execPath, "run", "scripts/rust/verify-company-physical.ts"],
  [process.execPath, "run", "scripts/rust/verify-company-vat.ts"],
  [process.execPath, "run", "scripts/rust/verify-sequences.ts"],
  [process.execPath, "run", "scripts/rust/verify-fiscal-calendar.ts"],
  [process.execPath, "run", "scripts/rust/verify-arithmetic.ts"],
  [process.execPath, "run", "scripts/rust/verify-manual-journal.ts"],
  [process.execPath, "run", "scripts/rust/verify-journal-preview.ts"],
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
