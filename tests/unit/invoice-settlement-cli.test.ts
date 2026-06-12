// Tests: src/cli/invoice.ts, src/cli.ts (invoice settlement CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice settle-bank CLI", () => {
  test("settles an invoice from bank receipt via CLI", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/customer-payment.csv`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "settle-bank", "--company", company, "--input", "examples/invoice-settlement.json"], {
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
    expect(parsed.appliedRules).toContain("DK-INVOICE-SETTLEMENT-001");
  });

  test("settles principal and claims from one combined receipt via CLI", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-settle-combined-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts invoice remind --company ${company} --invoice-number 2026-0001 --date 2026-06-26`.quiet();
    await Bun.$`bun run src/cli.ts invoice post-reminder --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts invoice compensation --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20`.quiet();
    await Bun.$`bun run src/cli.ts invoice claim-compensation --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20`.quiet();
    await Bun.$`bun run src/cli.ts invoice post-compensation --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts invoice interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();
    await Bun.$`bun run src/cli.ts invoice claim-interest --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20 --reference-rate 2.2`.quiet();
    await Bun.$`bun run src/cli.ts invoice post-interest --company ${company} --invoice-number 2026-0001`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/customer-payment-combined.csv`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "settle-bank", "--company", company, "--input", "examples/invoice-settlement-combined.json"], {
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
    expect(parsed.appliedRules).toContain("DK-INVOICE-COMBINED-SETTLEMENT-001");
    expect(parsed.claimAmount).toBe(411.75);
    expect(parsed.claimOpenBalance).toBe(0);
  });
});
