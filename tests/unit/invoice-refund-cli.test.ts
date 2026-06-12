// Tests: src/cli/invoice.ts, src/cli.ts (invoice refund CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice refund CLI", () => {
  test("refunds a credited invoice via CLI", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-refund-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --document-id 1`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/customer-payment.csv`.quiet();
    await Bun.$`bun run src/cli.ts invoice settle-bank --company ${company} --input examples/invoice-settlement.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice credit-note --company ${company} --input examples/credit-note.json`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/customer-refund.csv`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "refund-bank", "--company", company, "--input", "examples/invoice-refund.json"], {
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
    expect(parsed.appliedRules).toContain("DK-INVOICE-REFUND-001");
    expect(parsed.remainingCreditBalance).toBe(0);
  });
});
