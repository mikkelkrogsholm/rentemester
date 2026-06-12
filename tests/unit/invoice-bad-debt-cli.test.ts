// Tests: src/cli/invoice.ts, src/cli.ts (invoice bad-debt CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice write-off-bad-debt CLI", () => {
  test("writes off a posted invoice via CLI", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bad-debt-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "write-off-bad-debt", "--company", company, "--input", "examples/invoice-bad-debt-writeoff.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.appliedRules).toContain("DK-INVOICE-BAD-DEBT-WRITEOFF-001");
    expect(parsed.appliedRules).toContain("DK-VAT-BAD-DEBT-001");
    expect(parsed.openBalance).toBe(0);
  });
});
