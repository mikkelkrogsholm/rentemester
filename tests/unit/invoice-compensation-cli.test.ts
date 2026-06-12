// Tests: src/cli/invoice.ts, src/cli.ts (invoice compensation CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice compensation CLI", () => {
  test("posts a registered statutory fixed compensation claim to the ledger", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-post-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice-commercial.json");
    const paymentInput = join(root, "partial-payment.json");

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
    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice claim-compensation --company ${company} --invoice-number 2026-0001 --as-of 2026-06-20`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "post-compensation", "--company", company, "--invoice-number", "2026-0001"], {
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
    expect(parsed.compensationAmountDkk).toBe(310);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001");
  });

  test("registers a statutory fixed compensation claim for an overdue commercial invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-register-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice-commercial.json");
    const paymentInput = join(root, "partial-payment.json");

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
    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "claim-compensation", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-20"], {
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
    expect(parsed.claimId).toBeDefined();
    expect(parsed.compensationAmountDkk).toBe(310);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-REGISTER-001");
  });

  test("assesses statutory fixed compensation eligibility for an overdue commercial invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-comp-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice-commercial.json");
    const paymentInput = join(root, "partial-payment.json");

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
    writeFileSync(paymentInput, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${paymentInput}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "compensation", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-20"], {
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
    expect(parsed.eligible).toBe(true);
    expect(parsed.compensationAmountDkk).toBe(310);
    expect(parsed.appliedRules).toContain("DK-INVOICE-LATE-COMPENSATION-001");
  });
});
