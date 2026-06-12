// Tests: src/core/reconciliation.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { buildBankReconciliationReport, listBankTransactions } from "../../src/core/reconciliation";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("bank reconciliation", () => {
  test("shows matched and unmatched bank transactions for a period", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reconcile-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-reconcile-inbox-"));
    const csv = join(root, "transactions.csv");
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-05-16,2026-05-16,Software payment,-1250,DKK,REF-1",
      "2026-05-18,2026-05-18,Customer payment,2500,DKK,REF-2"
    ].join("\n"));
    writeFileSync(sourceFile, "Invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = importBankCsv(db, root, csv);
    expect(bank.ok).toBe(true);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-REC-1",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bank transfer"
    });
    expect(doc.ok).toBe(true);

    const firstBank = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-1'").get() as { id: number };
    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Software expense from bank payment",
      sourceBankTransactionId: firstBank.id,
      documentId: doc.documentId!,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 }
      ]
    });
    expect(posted.ok).toBe(true);

    const report = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31");
    expect(report.ok).toBe(true);
    expect(report.matchedCount).toBe(1);
    expect(report.unmatchedCount).toBe(1);
    expect(report.matched[0].journalEntryNo).toBe(posted.entryNo);
    expect(report.unmatched[0].text).toBe("Customer payment");
    expect(report.matchedAmountTotal).toBe(-1250);
    expect(report.unmatchedAmountTotal).toBe(2500);

    const unmatchedOnly = buildBankReconciliationReport(db, "2026-05-01", "2026-05-31", { status: "unmatched", textMatch: "customer" });
    expect(unmatchedOnly.ok).toBe(true);
    expect(unmatchedOnly.matchedCount).toBe(0);
    expect(unmatchedOnly.unmatchedCount).toBe(1);
    expect(unmatchedOnly.unmatched[0].text).toBe("Customer payment");

    const matchedOnly = listBankTransactions(db, { status: "matched", from: "2026-05-01", to: "2026-05-31", textMatch: "software" });
    expect(matchedOnly.ok).toBe(true);
    expect(matchedOnly.rows).toHaveLength(1);
    expect(matchedOnly.rows[0]).toMatchObject({ text: "Software payment", reconciliationStatus: "matched" });

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
