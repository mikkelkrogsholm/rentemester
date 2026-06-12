// Tests: src/cli/vat.ts, src/cli.ts (VAT CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("vat report CLI", () => {
  test("returns a VAT report for a company period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vatcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "vat", "report", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"], {
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
    expect(parsed.inputVat).toBe(250);
    expect(parsed.netVatPayable).toBe(-250);
    expect(parsed.warnings).toEqual([]);
    expect(parsed.journalEntryCount).toBe(1);
    expect(parsed.totalJournalEntryCount).toBe(1);
  });
});
