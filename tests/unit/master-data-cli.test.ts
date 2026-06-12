// Tests: src/cli/customer.ts, src/cli/vendor.ts, src/cli.ts (customer/vendor master-data CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { companyPaths } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";

import { cleanupDir } from "../helpers/cleanup";
async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("master-data CLI", () => {
  test("creates a customer and issues an invoice via --customer-id", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-customer-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice.json");

    writeFileSync(invoiceInput, JSON.stringify({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-18",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
      dueDate: "2026-06-17"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = await runCli(["customer", "create", "--company", company, "--name", "Kunde A/S", "--address", "Købervej 9, 8000 Aarhus C", "--cvr", "DK87654321", "--email", "kunde@example.com", "--ean", "5790000000001", "--payment-terms", "14"]);
    expect(created.exitCode).toBe(0);
    const createdJson = JSON.parse(created.stdout);
    expect(createdJson.ok).toBe(true);
    const customerId = createdJson.customerId;

    const listed = await runCli(["customer", "list", "--company", company]);
    expect(listed.exitCode).toBe(0);
    const listedJson = JSON.parse(listed.stdout);
    expect(listedJson.count).toBe(1);
    expect(listedJson.rows[0]).toMatchObject({ name: "Kunde A/S", vatOrCvr: "DK87654321", eanNumber: "5790000000001", paymentTermsDays: 14 });

    const issued = await runCli(["invoice", "issue", "--company", company, "--input", invoiceInput, "--customer-id", String(customerId)]);
    expect(issued.exitCode).toBe(0);
    const issuedJson = JSON.parse(issued.stdout);
    expect(issuedJson.ok).toBe(true);

    const db = openDb(companyPaths(company).db);
    migrate(db);
    const row = db.query("SELECT payload_json FROM documents WHERE id = ?").get(issuedJson.documentId) as { payload_json: string };
    const payload = JSON.parse(row.payload_json);
    db.close();

    cleanupDir(root);
    expect(payload.buyer).toEqual({
      name: "Kunde A/S",
      address: "Købervej 9, 8000 Aarhus C",
      vatOrCvr: "DK87654321",
      eanNumber: "5790000000001",
      publicRecipient: true,
    });
    expect(payload.dueDate).toBe("2026-06-17");
  });

  test("rejects invalid customer EAN numbers", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-customer-cli-bad-ean-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = await runCli(["customer", "create", "--company", company, "--name", "Kunde A/S", "--ean", "57900012"]);

    cleanupDir(root);
    expect(created.exitCode).toBe(1);
    expect(JSON.parse(created.stdout)).toMatchObject({
      ok: false,
      errors: ["eanNumber must be 13 digits"],
    });
  });

  test("creates a vendor and ingests a document via --vendor-id", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vendor-cli-"));
    const company = join(root, "company");
    const sourceFile = join(root, "vendor.txt");
    const metadataFile = join(root, "metadata.json");

    writeFileSync(sourceFile, "Invoice V-100\n1250 DKK\n");
    writeFileSync(metadataFile, JSON.stringify({
      source: "email",
      issueDate: "2026-05-18",
      invoiceNo: "V-100",
      deliveryDescription: "Software subscription",
      amountIncVat: 1250,
      currency: "DKK",
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const created = await runCli(["vendor", "create", "--company", company, "--name", "Leverandør ApS", "--address", "Sælgervej 1, 2100 København Ø", "--cvr", "DK11223344", "--expense-account", "3000", "--default-vat", "standard"]);
    expect(created.exitCode).toBe(0);
    const createdJson = JSON.parse(created.stdout);
    expect(createdJson.ok).toBe(true);
    const vendorId = createdJson.vendorId;

    const listed = await runCli(["vendor", "list", "--company", company]);
    expect(listed.exitCode).toBe(0);
    const listedJson = JSON.parse(listed.stdout);
    expect(listedJson.count).toBe(1);
    expect(listedJson.rows[0]).toMatchObject({ name: "Leverandør ApS", vatOrCvr: "DK11223344", defaultExpenseAccount: "3000" });

    const ingested = await runCli(["documents", "ingest", "--company", company, "--file", sourceFile, "--metadata", metadataFile, "--vendor-id", String(vendorId)]);
    expect(ingested.exitCode).toBe(0);
    const ingestedJson = JSON.parse(ingested.stdout);
    expect(ingestedJson.ok).toBe(true);

    const db = openDb(companyPaths(company).db);
    migrate(db);
    const row = db.query("SELECT sender_name, sender_address, sender_vat_cvr FROM documents WHERE id = ?").get(ingestedJson.documentId) as { sender_name: string; sender_address: string; sender_vat_cvr: string };
    db.close();

    cleanupDir(root);
    expect(row).toEqual({
      sender_name: "Leverandør ApS",
      sender_address: "Sælgervej 1, 2100 København Ø",
      sender_vat_cvr: "DK11223344",
    });
  });
});
