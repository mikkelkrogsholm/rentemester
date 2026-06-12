// Tests: src/cli/invoice.ts, src/cli.ts (invoice CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice validate CLI", () => {
  test("returns exit code 0 for valid invoice input", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-invoice-"));
    const file = join(dir, "invoice.json");
    writeFileSync(file, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0100",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring" }],
      totals: { netAmount: 400, vatRate: 0.25, vatAmount: 100, grossAmount: 500 },
      currency: "DKK"
    }, null, 2));

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "validate", "--input", file], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(dir);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
  });
});
