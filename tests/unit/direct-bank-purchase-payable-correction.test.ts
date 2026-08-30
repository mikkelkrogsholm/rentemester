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
import { bookExpenseFromBank } from "../../src/core/expense-booking";
import { planDirectBankPurchasePayableCorrection, applyDirectBankPurchasePayableCorrection } from "../../src/core/direct-bank-purchase-payable-correction";
import { verifyAuditChain } from "../../src/core/ledger";
import { computePeriodCloseReadiness } from "../../src/core/period-close-readiness";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";

function fixture(bankDate = "2026-07-02") {
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
  writeFileSync(csv, `transaction_date,booking_date,text,amount,currency,reference\n${bankDate},${bankDate},SYNTHETIC,-509,DKK,SYN-509\n`);
  expect(importBankCsv(db, root, csv).ok).toBe(true);
  const bankId = (db.query("SELECT id FROM bank_transactions WHERE reference='SYN-509'").get() as { id:number }).id;
  db.run("UPDATE bank_transactions SET balance_after=-509 WHERE id=?", bankId);
  return { root, inbox, db, documentId: document.documentId!, bankId };
}

describe("direct-bank purchase payable correction temporal invariants (#594)", () => {
  test("supports same-period settlement without duplicating the reconciliation", () => {
    const f = fixture("2026-06-20");
    try {
      expect(bookExpenseFromBank(f.db, { documentId:f.documentId, bankTransactionId:f.bankId, expenseAccountNo:"3000", vatTreatment:"standard" }).ok).toBe(true);
      const input = { documentId:f.documentId, bankTransactionId:f.bankId, billDate:"2026-06-16", dueDate:"2026-06-30", expenseAccountNo:"3000", vatTreatment:"standard" as const };
      const plan = planDirectBankPurchasePayableCorrection(f.db,input);
      expect(plan.ok).toBe(true);
      if (!plan.ok) return;
      const applied=applyDirectBankPurchasePayableCorrection(f.db,{...input,planHash:plan.plan.planHash,reason:"Canonical payable lifecycle",actor:"agent:codex",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true});
      expect(applied.ok).toBe(true);
      expect(f.db.query("SELECT COUNT(*) AS n FROM bank_journal_reconciliations WHERE bank_transaction_id=?").get(f.bankId)).toEqual({n:1});
      expect(getPayableStatus(f.db,applied.payableId!,"2026-06-19")).toMatchObject({openBalance:509,status:"open"});
      expect(getPayableStatus(f.db,applied.payableId!,"2026-06-20")).toMatchObject({openBalance:0,status:"paid"});
    } finally { f.db.close(); rmSync(f.root,{recursive:true,force:true}); rmSync(f.inbox,{recursive:true,force:true}); }
  });

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

  test("converts a mixed-VAT direct bank purchase append-only and replays safely", () => {
    const f = fixture();
    try {
      const direct = bookExpenseFromBank(f.db, { documentId:f.documentId, bankTransactionId:f.bankId, expenseAccountNo:"3000", vatTreatment:"standard" });
      expect(direct.ok, direct.errors.join("; ")).toBe(true);
      const input = { documentId:f.documentId, bankTransactionId:f.bankId, billDate:"2026-06-16", dueDate:"2026-07-16", expenseAccountNo:"3000", vatTreatment:"standard" as const };
      const planned = planDirectBankPurchasePayableCorrection(f.db, input);
      expect(planned.ok, planned.errors.join("; ")).toBe(true);
      if (!planned.ok) return;
      f.db.run("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-06-01','2026-06-30','vat_period','open')");
      expect(applyDirectBankPurchasePayableCorrection(f.db, { ...input, planHash:planned.plan.planHash, reason:"Stale review", actor:"agent:codex", principal:{kind:"service-account",subjectId:"svc-test"}, confirm:true })).toMatchObject({ok:false,errors:["PLAN_HASH_MISMATCH"]});
      const reviewed = planDirectBankPurchasePayableCorrection(f.db, input);
      expect(reviewed.ok).toBe(true);
      if (!reviewed.ok) return;
      const applied = applyDirectBankPurchasePayableCorrection(f.db, { ...input, planHash:reviewed.plan.planHash, reason:"Correct period cut-off", actor:"agent:codex", principal:{kind:"service-account",subjectId:"svc-test"}, confirm:true });
      expect(applied.ok, applied.errors.join("; ")).toBe(true);
      if (!applied.ok) return;
      expect(getPayableStatus(f.db, applied.payableId!, "2026-06-30")).toMatchObject({openBalance:509,paidAmount:0,status:"open"});
      expect(getPayableStatus(f.db, applied.payableId!, "2026-07-02")).toMatchObject({openBalance:0,paidAmount:509,status:"paid"});
      expect(f.db.query("SELECT COUNT(*) AS n FROM bank_journal_reconciliations WHERE bank_transaction_id=?").get(f.bankId)).toEqual({n:1});
      expect(f.db.query("SELECT transaction_date FROM journal_entries WHERE id=?").get(applied.settlementJournalEntryId)).toEqual({transaction_date:"2026-07-02"});
      const h1=computePeriodCloseReadiness(f.db,{periodStart:"2026-01-01",periodEnd:"2026-06-30",companyRoot:f.root});
      const current=computePeriodCloseReadiness(f.db,{periodStart:"2026-01-01",periodEnd:"2026-07-02",companyRoot:f.root});
      expect(h1.items.find(item=>item.code==="DKK_CONTROL_ACCOUNTS")?.status).toBe("passed");
      expect(current.items.find(item=>item.code==="DKK_CONTROL_ACCOUNTS")?.status).toBe("passed");
      const replay=applyDirectBankPurchasePayableCorrection(f.db,{...input,planHash:reviewed.plan.planHash,reason:"Correct period cut-off",actor:"agent:codex",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true});
      expect(replay).toMatchObject({ok:true,idempotent:true,id:applied.id});
      const conflict=applyDirectBankPurchasePayableCorrection(f.db,{...input,planHash:"0".repeat(64),reason:"Changed",actor:"agent:codex",principal:{kind:"service-account",subjectId:"svc-test"},confirm:true});
      expect(conflict).toMatchObject({ok:false,errors:["IDEMPOTENCY_PAYLOAD_CONFLICT"]});
      expect(()=>f.db.run("UPDATE direct_bank_purchase_payable_corrections SET reason='changed' WHERE id=?",applied.id)).toThrow("append-only");
      expect(()=>f.db.run("DELETE FROM direct_bank_purchase_payable_corrections WHERE id=?",applied.id)).toThrow("append-only");
      f.db.exec("DROP TRIGGER direct_bank_purchase_payable_corrections_no_update; DROP TRIGGER direct_bank_purchase_payable_corrections_no_delete; DROP TRIGGER bank_reconciliation_correction_events_guard_insert;");
      migrate(f.db);
      expect(()=>f.db.run("UPDATE direct_bank_purchase_payable_corrections SET reason='changed' WHERE id=?",applied.id)).toThrow("append-only");
      expect(f.db.query("SELECT 1 AS present FROM sqlite_master WHERE type='trigger' AND name='bank_reconciliation_correction_events_guard_insert'").get()).toEqual({present:1});
      expect(verifyAuditChain(f.db).ok).toBe(true);
      expect(f.db.query("PRAGMA integrity_check").get()).toEqual({integrity_check:"ok"});
      const restoredRoot=mkdtempSync(join(tmpdir(),"rentemester-direct-payable-restored-"));
      try {
        const backup=createSystemBackup(f.db,f.root,{createdAt:"2026-07-03T02:00:00.000Z"});
        expect(backup.ok).toBe(true);
        const restored=restoreSystemBackup({backupDir:backup.backupDir!,targetCompanyRoot:restoredRoot});
        expect(restored.ok,restored.errors.join("; ")).toBe(true);
        const restoredDb=openDb(restored.restoredDbPath!);
        try {
          expect(restoredDb.query("SELECT document_id,bank_transaction_id,plan_hash FROM direct_bank_purchase_payable_corrections WHERE id=?").get(applied.id)).toMatchObject({document_id:f.documentId,bank_transaction_id:f.bankId,plan_hash:reviewed.plan.planHash});
          expect(verifyAuditChain(restoredDb,{companyRoot:restoredRoot}).ok).toBe(true);
        } finally { restoredDb.close(); }
      } finally { rmSync(restoredRoot,{recursive:true,force:true}); }
    } finally { f.db.close(); rmSync(f.root,{recursive:true,force:true}); rmSync(f.inbox,{recursive:true,force:true}); }
  });
});
