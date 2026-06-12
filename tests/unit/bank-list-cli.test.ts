// Tests: src/cli/bank.ts, src/cli.ts (bank list CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("bank list CLI", () => {
  test("filters bank transactions by reconciliation status and text", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-list-"));
    const company = join(root, "company");
    const vendorDoc = join(root, "vendor.txt");
    writeFileSync(vendorDoc, "Invoice\n1250 DKK\n");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file ${vendorDoc} --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "bank", "list",
      "--company", company,
      "--status", "unmatched",
      "--text-match", "customer",
      "--format", "json",
    ], {
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
    expect(parsed.count).toBe(1);
    expect(parsed.rows[0]).toMatchObject({ text: "Customer payment", reconciliationStatus: "unmatched" });
  });
});
