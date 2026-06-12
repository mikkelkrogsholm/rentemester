// Tests: src/cli/invoice.ts, src/cli.ts (invoice reminders CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice reminder CLI", () => {
  test("posts a registered statutory reminder fee to the ledger", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-post-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice-commercial.json");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice remind --company ${company} --invoice-number 2026-0001 --date 2026-06-26`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "post-reminder", "--company", company, "--invoice-number", "2026-0001"], {
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
    expect(parsed.entryId).toBeDefined();
    expect(parsed.feeAmount).toBe(100);
    expect(parsed.appliedRules).toContain("DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001");
  });

  test("registers a statutory reminder fee", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-reminder-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice-commercial.json");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "remind", "--company", company, "--invoice-number", "2026-0001", "--date", "2026-06-26"], {
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
    expect(parsed.reminderSequence).toBe(1);
    expect(parsed.feeAmount).toBe(100);
    expect(parsed.appliedRules).toContain("DK-INVOICE-REMINDER-FEE-001");
  });
});
