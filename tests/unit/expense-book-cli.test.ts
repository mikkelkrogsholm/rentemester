// Tests: src/cli/expense.ts, src/cli.ts (expense book CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("expense book CLI", () => {
  test("books a vendor expense directly from document and bank ids", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-cli-"));
    const company = join(root, "company");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-cli-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    const metadataFile = join(root, "vendor.metadata.json");
    const bankCsv = join(root, "bank.csv");
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");
    writeFileSync(metadataFile, JSON.stringify({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "CLI-EXP-1",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Software ApS", address: "SaaSvej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer"
    }, null, 2));
    writeFileSync(bankCsv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,SOFTWARE APS,-1250,DKK,REF-CLI-1"
    ].join("\n"));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file ${sourceFile} --metadata ${metadataFile}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file ${bankCsv}`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "expense", "book",
      "--company", company,
      "--document-id", "1",
      "--bank-transaction-id", "1",
      "--expense-account", "3000"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    cleanupDir(inbox);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.vatTreatment).toBe("standard");
    expect(parsed.grossAmount).toBe(1250);
    expect(parsed.entryNo).toBe("2026-00001");
  });


  test("books a foreign-currency vendor expense from a DKK bank settlement", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-expense-book-cli-fx-"));
    const company = join(root, "company");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-expense-book-cli-fx-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    const metadataFile = join(root, "vendor.metadata.json");
    const bankCsv = join(root, "bank.csv");
    writeFileSync(sourceFile, "Invoice\n100 EUR\n");
    writeFileSync(metadataFile, JSON.stringify({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "CLI-FX-1",
      deliveryDescription: "Cloud subscription",
      amountIncVat: 100,
      currency: "EUR",
      sender: { name: "Cloud Vendor GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Card payment"
    }, null, 2));
    writeFileSync(bankCsv, [
      "transaction_date,booking_date,text,amount,currency,fx_rate_to_dkk,reference",
      "2026-05-16,2026-05-16,CLOUD VENDOR,-746,DKK,7.46,REF-CLI-FX-1"
    ].join("\n"));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file ${sourceFile} --metadata ${metadataFile}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file ${bankCsv}`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "expense", "book",
      "--company", company,
      "--document-id", "1",
      "--bank-transaction-id", "1",
      "--expense-account", "3000",
      "--vat-treatment", "exempt"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    cleanupDir(inbox);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.vatTreatment).toBe("exempt");
    expect(parsed.grossAmount).toBe(100);
    expect(parsed.netAmount).toBe(746);
  });
});
