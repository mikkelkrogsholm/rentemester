// Tests: src/cli/bank.ts, src/cli.ts (reconciliation CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("bank reconciliation CLI", () => {
  test("returns matched and unmatched counts for a period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reconcile-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "reconcile", "bank", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"], {
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
    expect(parsed.matchedCount).toBe(0);
    expect(parsed.unmatchedCount).toBe(2);
  });

  test("filters reconciliation report to unmatched rows", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reconcile-cli-filter-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "reconcile", "bank", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31", "--status", "unmatched", "--text-match", "customer"], {
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
    expect(parsed.unmatchedCount).toBe(1);
    expect(parsed.unmatched[0].text).toBe("Customer payment");
    expect(parsed.matched).toEqual([]);
  });
});
