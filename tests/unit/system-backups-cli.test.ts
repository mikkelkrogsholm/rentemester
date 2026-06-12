// Tests: src/cli/system.ts, src/cli.ts (system backups CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("system backup CLI", () => {
  test("creates a backup and reports compliant status", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-backup-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();

    const backupProc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "backup", "--company", company, "--at", "2026-05-17T02:09:00.000Z"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const backupStdout = await new Response(backupProc.stdout).text();
    const backupStderr = await new Response(backupProc.stderr).text();
    const backupExitCode = await backupProc.exited;

    const statusProc = Bun.spawn(["bun", "run", "src/cli.ts", "system", "backup-status", "--company", company, "--as-of", "2026-05-17T02:10:00.000Z"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const statusStdout = await new Response(statusProc.stdout).text();
    const statusStderr = await new Response(statusProc.stderr).text();
    const statusExitCode = await statusProc.exited;

    cleanupDir(root);

    expect({ backupExitCode, backupStderr }).toEqual({ backupExitCode: 0, backupStderr: "" });
    expect({ statusExitCode, statusStderr }).toEqual({ statusExitCode: 0, statusStderr: "" });

    const backupParsed = JSON.parse(backupStdout);
    const statusParsed = JSON.parse(statusStdout);
    expect(backupParsed.ok).toBe(true);
    expect(backupParsed.backupId).toBe("backup-20260517T020900Z");
    expect(statusParsed.ok).toBe(true);
    expect(statusParsed.backupDue).toBe(false);
    expect(statusParsed.appliedRules).toContain("DK-BOOKKEEPING-BACKUP-001");
  });
});
