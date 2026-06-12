// Tests: src/cli/system.ts, src/cli.ts (system restore CLI)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("system restore CLI", () => {
  test("restores a created backup into a fresh company folder", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-cli-"));
    const company = join(root, "company");
    const restored = join(root, "restored-company");
    const backupDir = join(company, "backups", "backup-20260517T023900Z");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:39:00.000Z`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "restore-backup", "--backup-dir", backupDir, "--target-company", restored, "--confirm", "yes"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.backupId).toBe("backup-20260517T023900Z");
    expect(existsSync(join(restored, "data", "ledger.sqlite"))).toBe(true);

    cleanupDir(root);
  });

  test("refuses to restore without --confirm yes and writes nothing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-cli-noconfirm-"));
    const company = join(root, "company");
    const restored = join(root, "restored-company");
    const backupDir = join(company, "backups", "backup-20260517T023900Z");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:39:00.000Z`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "restore-backup", "--backup-dir", backupDir, "--target-company", restored], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    // ok:false result → exit code 1, and the destructive restore never ran.
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.join(" ")).toContain("--confirm yes");
    expect(existsSync(join(restored, "data", "ledger.sqlite"))).toBe(false);

    cleanupDir(root);
  });

  // #283: restoring a backup into a brand-new company path is the normal
  // case. The target has no `config/policy.yaml` yet (a fresh restore cannot
  // have one), so an explicit, correctly-formatted `--actor` must be accepted
  // — the actor allowlist cannot be enforced against a file that does not
  // exist. Previously every explicit `--actor` was rejected with a misleading
  // "not in config/policy.yaml actor_allowlist" error, while omitting
  // `--actor` (a derived actor) worked — so doing the right thing was blocked.
  test("accepts an explicit --actor when restoring into a fresh directory (#283)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-cli-actor-"));
    const company = join(root, "company");
    const restored = join(root, "restored-company");
    const backupDir = join(company, "backups", "backup-20260517T023900Z");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:39:00.000Z`.quiet();

    const proc = Bun.spawn(
      [
        "bun",
        "run",
        "src/cli.ts",
        "system",
        "restore-backup",
        "--backup-dir",
        backupDir,
        "--target-company",
        restored,
        "--confirm",
        "yes",
        "--actor",
        "user:restore-operator",
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.backupId).toBe("backup-20260517T023900Z");
    expect(existsSync(join(restored, "data", "ledger.sqlite"))).toBe(true);

    cleanupDir(root);
  });

  test("rejects --confirm with a non-yes value", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-cli-badconfirm-"));
    const company = join(root, "company");
    const restored = join(root, "restored-company");
    const backupDir = join(company, "backups", "backup-20260517T023900Z");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:39:00.000Z`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "restore-backup", "--backup-dir", backupDir, "--target-company", restored, "--confirm", "true"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(existsSync(join(restored, "data", "ledger.sqlite"))).toBe(false);

    cleanupDir(root);
  });
});
