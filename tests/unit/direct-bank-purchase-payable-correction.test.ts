import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { importBankCsv } from "../../src/core/bank";
import { buildPayablesList, getPayableStatus, payPayableFromBank, registerPayable } from "../../src/core/payables";

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-direct-purchase-payable-"));
  const inbox = mkdtempSync(join(tmpdir(), "rentemester-direct-purchase-payable-inbox-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db); seedAccounts(db);
  db.run("INSERT INTO companies (id,name,country,currency,cvr,address,postal_code,city,vat_period_type) VALUES (1,'Synthetic company','DK','DKK','DK12345678','Test 2','1000','Testby','quarter')");
  const source = join(inbox, "bill.txt"); writeFileSync(source, "synthetic bill");
  const document = ingestDocument(db, root, source, {
    source: "email", issueDate: "2026-06-16", invoiceNo: "SYN-509", amountIncVat: 509,
    vatAmount: 86, currency: "DKK", deliveryDescription: "synthetic mixed purchase",
    sender: { name: "Synthetic supplier", address: "Test 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Synthetic company", address: "Test 2", vatOrCvr: "DK12345678" },
    paymentDetails: "bank", danishSimplifiedPurchaseInvoice: true, purchaseVatLines: [
      { classification: "dk_purchase_25", netAmount: 344, vatAmount: 86 },
      { classification: "exempt", netAmount: 79, vatAmount: 0 },
    ],
  });
  expect(document.ok).toBe(true);
  const csv = join(root, "bank.csv");
  writeFileSync(csv, "transaction_date,booking_date,text,amount,currency,reference\n2026-07-02,2026-07-02,SYNTHETIC,-509,DKK,SYN-509\n");
  expect(importBankCsv(db, root, csv).ok).toBe(true);
  const bankId = (db.query("SELECT id FROM bank_transactions WHERE reference='SYN-509'").get() as { id:number }).id;
  return { root, inbox, db, documentId: document.documentId!, bankId };
}

describe("direct-bank purchase payable correction temporal invariants (#594)", () => {
  test("uses the immutable document invoice date and presents bill/payment balances as-of", () => {
    const f = fixture();
    try {
      const wrongDate = registerPayable(f.db, { documentId: f.documentId, billDate: "2026-06-17", dueDate: "2026-07-16", expenseAccountNo: "3000" });
      expect(wrongDate.ok).toBe(false);
      expect(wrongDate.errors.join(" ")).toContain("invoice_date");
      const bill = registerPayable(f.db, { documentId: f.documentId, billDate: "2026-06-16", dueDate: "2026-07-16", expenseAccountNo: "3000" });
      expect(bill.ok).toBe(true);
      expect(getPayableStatus(f.db, bill.payableId!, "2026-06-30")).toMatchObject({ openBalance: 509, paidAmount: 0, status: "open" });
      expect(buildPayablesList(f.db, { asOfDate: "2026-06-15" }).count).toBe(0);
      const paid = payPayableFromBank(f.db, { payableId: bill.payableId!, bankTransactionId: f.bankId });
      expect(paid.ok).toBe(true);
      expect(getPayableStatus(f.db, bill.payableId!, "2026-06-30")).toMatchObject({ openBalance: 509, paidAmount: 0, status: "open" });
      expect(getPayableStatus(f.db, bill.payableId!, "2026-07-02")).toMatchObject({ openBalance: 0, paidAmount: 509, status: "paid" });
    } finally { f.db.close(); rmSync(f.root, { recursive:true, force:true }); rmSync(f.inbox, { recursive:true, force:true }); }
  });
});
