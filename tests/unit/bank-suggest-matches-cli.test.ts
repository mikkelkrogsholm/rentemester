// Tests: src/cli/bank.ts, src/cli.ts (bank suggest-matches CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { ingestDocument } from "../../src/core/documents";
import { importBankCsv } from "../../src/core/bank";

import { cleanupDir } from "../helpers/cleanup";
function invoicePayload(overrides: Record<string, unknown> = {}) {
  return {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    invoiceNumber: "2026-0001",
    seller: {
      name: "Rentemester ApS",
      address: "Testvej 1, 2100 København Ø",
      vatOrCvr: "DK12345678",
    },
    buyer: {
      name: "Kunde A/S",
      address: "Købervej 9, 8000 Aarhus C",
      vatOrCvr: "DK87654321",
    },
    lines: [
      {
        description: "Bogføring og momsafstemning",
        quantity: 1,
        unitPriceExVat: 1000,
        lineTotalExVat: 1000,
      },
    ],
    totals: {
      netAmount: 1000,
      vatRate: 0.25,
      vatAmount: 250,
      grossAmount: 1250,
    },
    currency: "DKK",
    dueDate: "2026-06-15",
    ...overrides,
  };
}

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

describe("bank suggest-matches CLI", () => {
  test("suggests issued-invoice and purchase-sale matches for unmatched bank transactions", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-suggest-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-bank-suggest-inbox-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const invoice = issueInvoice(db, root, invoicePayload());
    expect(invoice.ok).toBe(true);

    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Software invoice\n1250 DKK\n");
    const purchase = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "SUP-1001",
      deliveryDescription: "Software subscription",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Stripe Payments Ltd", address: "1 Market St", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Stripe payout",
    });
    expect(purchase.ok).toBe(true);

    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-20,2026-05-20,Customer payment 2026-0001 Kunde A/S,1250,DKK,INV-2026-0001",
      "2026-05-16,2026-05-16,Stripe payout SUP-1001,-1250,DKK,STRIPE-1",
    ].join("\n"));
    const imported = importBankCsv(db, root, csv);
    expect(imported.ok).toBe(true);

    const invoiceBank = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-2026-0001'").get() as { id: number };
    db.close();

    const listed = await runCli(["bank", "suggest-matches", "--company", root, "--format", "json"]);
    const filtered = await runCli(["bank", "suggest-matches", "--company", root, "--bank-transaction-id", String(invoiceBank.id), "--max", "1", "--format", "json"]);

    cleanupDir(root);
    cleanupDir(inbox);

    expect(listed.exitCode).toBe(0);
    expect(listed.stderr).toBe("");
    const listedJson = JSON.parse(listed.stdout);
    expect(listedJson.count).toBe(2);

    const invoiceRow = listedJson.rows.find((row: any) => row.reference === "INV-2026-0001");
    expect(invoiceRow.suggestions[0]).toMatchObject({
      kind: "issued_invoice",
      invoiceNo: "2026-0001",
      customerName: "Kunde A/S",
    });
    expect(invoiceRow.suggestions[0].reasons.some((reason: string) => reason.includes("amount match"))).toBe(true);

    const purchaseRow = listedJson.rows.find((row: any) => row.reference === "STRIPE-1");
    expect(purchaseRow.suggestions[0]).toMatchObject({
      kind: "purchase_sale",
      invoiceNo: "SUP-1001",
      supplierName: "Stripe Payments Ltd",
    });
    expect(purchaseRow.suggestions[0].reasons.some((reason: string) => reason.includes("supplier token match"))).toBe(true);

    expect(filtered.exitCode).toBe(0);
    expect(filtered.stderr).toBe("");
    const filteredJson = JSON.parse(filtered.stdout);
    expect(filteredJson.count).toBe(1);
    expect(filteredJson.rows[0].bankTransactionId).toBe(invoiceBank.id);
    expect(filteredJson.rows[0].suggestions).toHaveLength(1);
    expect(filteredJson.rows[0].suggestions[0].invoiceNo).toBe("2026-0001");
  });

  test("matches an invoice when the bank amount is float-distinct but øre-equal", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-suggest-ore-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);

    // 1250.10 + 0.05 evaluates to 1250.1499999999999 in IEEE-754 — mathematically
    // equal to the 1250.15 invoice gross but float-distinct. Integer-øre
    // comparison (equalsDkk) must still recognise it as an amount match.
    const grossA = 1250.1;
    const grossB = 0.05;
    const bankAmount = grossA + grossB; // 1250.1499999999999
    expect(bankAmount).not.toBe(1250.15);

    const matchInvoice = issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0002",
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 1000.12, lineTotalExVat: 1000.12 }],
      totals: { netAmount: 1000.12, vatRate: 0.25, vatAmount: 250.03, grossAmount: 1250.15 },
    }));
    expect(matchInvoice.ok).toBe(true);

    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      `2026-05-20,2026-05-20,Customer payment 2026-0002 Kunde A/S,${bankAmount},DKK,INV-2026-0002`,
    ].join("\n"));
    const imported = importBankCsv(db, root, csv);
    expect(imported.ok).toBe(true);
    db.close();

    const listed = await runCli(["bank", "suggest-matches", "--company", root, "--format", "json"]);
    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    expect(listed.stderr).toBe("");
    const listedJson = JSON.parse(listed.stdout);
    const row = listedJson.rows.find((r: any) => r.reference === "INV-2026-0002");
    expect(row.suggestions[0]).toMatchObject({ kind: "issued_invoice", invoiceNo: "2026-0002" });
    expect(row.suggestions[0].reasons.some((reason: string) => reason.includes("amount match"))).toBe(true);
  });

  test("does not match an outgoing customer refund against the issued invoice (#154 limitation)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-suggest-refund-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const invoice = issueInvoice(db, root, invoicePayload({ invoiceNumber: "2026-0001" }));
    expect(invoice.ok).toBe(true);

    // A negative (outgoing) customer-refund row referencing the invoice number.
    // Reconciliation deliberately has no refund/credit-note matching path, so
    // this must produce zero suggestions rather than be mistaken for a bug.
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-22,2026-05-22,Customer refund 2026-0001 Kunde A/S,-1250,DKK,RFND-2026-0001",
    ].join("\n"));
    const imported = importBankCsv(db, root, csv);
    expect(imported.ok).toBe(true);
    db.close();

    const listed = await runCli(["bank", "suggest-matches", "--company", root, "--format", "json"]);
    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    expect(listed.stderr).toBe("");
    const listedJson = JSON.parse(listed.stdout);
    const refundRow = listedJson.rows.find((r: any) => r.reference === "RFND-2026-0001");
    expect(refundRow.suggestions).toHaveLength(0);
  });

  test("does not emit a crossing-threshold suggestion from an amount-only match", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-suggest-amt-"));
    const paths = ensureCompanyDirs(root);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    // Two distinct customers, both ApS-suffixed, same gross amount.
    const first = issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0001",
      buyer: { name: "Alfa Bogforing ApS", address: "Alfavej 1, 1000 Kobenhavn", vatOrCvr: "DK11111111" },
    }));
    expect(first.ok).toBe(true);
    const second = issueInvoice(db, root, invoicePayload({
      invoiceNumber: "2026-0002",
      buyer: { name: "Beta Revision ApS", address: "Betavej 2, 2000 Frederiksberg", vatOrCvr: "DK22222222" },
    }));
    expect(second.ok).toBe(true);

    // A deposit equal to both invoice balances, with no invoice number and
    // no customer-name overlap in the text/reference.
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-20,2026-05-20,Indbetaling,1250,DKK,DEPOSIT-1",
    ].join("\n"));
    const imported = importBankCsv(db, root, csv);
    expect(imported.ok).toBe(true);
    db.close();

    const listed = await runCli(["bank", "suggest-matches", "--company", root, "--format", "json"]);
    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    expect(listed.stderr).toBe("");
    const listedJson = JSON.parse(listed.stdout);
    const depositRow = listedJson.rows.find((row: any) => row.reference === "DEPOSIT-1");
    // Amount alone must not produce a crossing-threshold suggestion that an
    // agent would auto-apply: no corroborating invoice-no / name signal.
    expect(depositRow.suggestions).toHaveLength(0);
  });
});
