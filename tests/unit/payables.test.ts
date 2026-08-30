// Tests: src/core/payables.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry } from "../../src/core/ledger";
import { planDirectBankPurchasePayableCorrection, applyDirectBankPurchasePayableCorrection } from "../../src/core/direct-bank-purchase-payable-correction";
import { todayIsoDate } from "../../src/core/dates";
import {
  registerPayable,
  payPayableFromBank,
  getPayableStatus,
  buildPayablesList,
} from "../../src/core/payables";

function setup(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function ingestPurchase(
  db: ReturnType<typeof openDb>,
  root: string,
  inbox: string,
  name: string,
  invoiceNo: string,
  amountIncVat: number,
  vatAmount: number,
  issueDate = "2026-01-10",
) {
  const sourceFile = join(inbox, `${invoiceNo}.txt`);
  writeFileSync(sourceFile, `Invoice ${invoiceNo}\n${amountIncVat} DKK\n`);
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate,
    invoiceNo,
    deliveryDescription: "Leverandørydelse",
    amountIncVat,
    currency: "DKK",
    sender: { name, address: "Leverandørvej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount,
    paymentDetails: "Bank transfer",
  });
  expect(doc.ok).toBe(true);
  return doc.documentId!;
}

describe("payables (kreditorstyring)", () => {
  test("#594 corrects a mixed taxable/exempt June direct-bank purchase into a July-settled payable without rewriting evidence", () => {
    const { root, db } = setup("rentemester-payables-cross-period-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-cross-period-inbox-"));
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, "transaction_date,booking_date,text,amount,currency,reference\n2026-07-02,2026-07-02,SYNTHETIC SUPPLIER,-509,DKK,REF-594\n");
    expect(importBankCsv(db, root, csv).ok).toBe(true);
    const source = join(inbox, "V-594.txt"); writeFileSync(source, "Synthetic mixed VAT purchase");
    const ingested = ingestDocument(db, root, source, { source:"email",issueDate:"2026-06-16",invoiceNo:"V-594",deliveryDescription:"Synthetic",amountIncVat:509,currency:"DKK",sender:{name:"Synthetic Supplier",address:"Example 1",vatOrCvr:"DK11223344"},recipient:{name:"Synthetic Company",address:"Example 2",vatOrCvr:"DK12345678"},vatAmount:86,paymentDetails:"Bank",purchaseVatLines:[{classification:"dk_purchase_25",netAmount:344,vatAmount:86},{classification:"exempt",netAmount:79,vatAmount:0}] } as any);
    expect(ingested.ok).toBe(true);
    const bankId=(db.query("SELECT id FROM bank_transactions WHERE reference='REF-594'").get() as any).id;
    db.run("UPDATE bank_accounts SET ledger_account_no='2000'");
    const direct=postJournalEntry(db,{transactionDate:"2026-06-16",text:"legacy direct bank",documentId:ingested.documentId!,sourceBankTransactionId:bankId,lines:[{accountNo:"3000",debitAmount:344,vatCode:"DK_PURCHASE_25"},{accountNo:"3000",debitAmount:79},{accountNo:"4000",debitAmount:86},{accountNo:"2000",creditAmount:509}]});
    expect(direct.ok).toBe(true);
    const input={documentId:ingested.documentId!,bankTransactionId:bankId,billDate:"2026-06-16",dueDate:"2026-06-30",expenseAccountNo:"3000",vatTreatment:"standard" as const};
    expect(planDirectBankPurchasePayableCorrection(db,{...input,billDate:"2026-06-17"}).errors).toContain("BILL_DATE_MUST_EQUAL_DOCUMENT_INVOICE_DATE");
    const plan=planDirectBankPurchasePayableCorrection(db,input); expect(plan.ok).toBe(true); if(!plan.ok) throw new Error();
    expect(applyDirectBankPurchasePayableCorrection(db,{...input,planHash:"0".repeat(64),reason:"Synthetic timing correction",actor:"agent:test",principal:{kind:"user",subjectId:"synthetic-reviewer"},confirm:true}).errors).toContain("PLAN_HASH_MISMATCH");
    db.close(); rmSync(root,{recursive:true,force:true}); rmSync(inbox,{recursive:true,force:true});
  });
  test("registers a supplier bill as an open item and posts to Leverandørgæld", () => {
    const { root, db } = setup("rentemester-payables-register-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-register-inbox-"));
    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-1001", 1250, 250);

    const registered = registerPayable(db, {
      documentId,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccountNo: "3000",
    });

    expect(registered.ok).toBe(true);
    expect(registered.payableId).toBeGreaterThan(0);
    expect(registered.grossAmount).toBe(1250);
    expect(registered.netAmount).toBe(1000);
    expect(registered.vatAmount).toBe(250);

    // Bill-recognition entry: debit expense + købsmoms, credit Leverandørgæld.
    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(registered.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "3000", debit_amount: 1000, credit_amount: 0 },
      { account_no: "4000", debit_amount: 250, credit_amount: 0 },
      { account_no: "7000", debit_amount: 0, credit_amount: 1250 },
    ]);

    const status = getPayableStatus(db, registered.payableId!);
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(1250);
    expect(status.paidAmount).toBe(0);
    expect(status.status).toBe("open");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("matches an outgoing bank payment against a payable and closes it", () => {
    const { root, db } = setup("rentemester-payables-pay-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-pay-inbox-"));
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-02-05,2026-02-05,SOFTWARE APS,-1250,DKK,REF-PAY-1",
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-1001", 1250, 250);
    const registered = registerPayable(db, {
      documentId,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccountNo: "3000",
    });
    expect(registered.ok).toBe(true);

    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-PAY-1'").get() as { id: number };
    const paid = payPayableFromBank(db, {
      payableId: registered.payableId!,
      bankTransactionId: bankRow.id,
    });
    expect(paid.ok).toBe(true);
    expect(paid.openBalance).toBe(0);

    // Settlement entry: debit Leverandørgæld, credit bank.
    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(paid.journalEntryId!) as any[];
    expect(lines).toEqual([
      { account_no: "7000", debit_amount: 1250, credit_amount: 0 },
      { account_no: "2000", debit_amount: 0, credit_amount: 1250 },
    ]);

    const status = getPayableStatus(db, registered.payableId!);
    expect(status.openBalance).toBe(0);
    expect(status.status).toBe("paid");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("rejects a payment that exceeds the open payable balance", () => {
    const { root, db } = setup("rentemester-payables-overpay-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-overpay-inbox-"));
    const csv = join(root, "transactions.csv");
    writeFileSync(csv, [
      "transaction_date,booking_date,text,amount,currency,reference",
      "2026-02-05,2026-02-05,SOFTWARE APS,-2000,DKK,REF-OVER-1",
    ].join("\n"));
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-1001", 1250, 250);
    const registered = registerPayable(db, {
      documentId,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccountNo: "3000",
    });
    const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-OVER-1'").get() as { id: number };
    const paid = payPayableFromBank(db, {
      payableId: registered.payableId!,
      bankTransactionId: bankRow.id,
    });
    expect(paid.ok).toBe(false);
    expect(paid.errors.join(" ")).toContain("exceeds");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("rejects a second registration of the same purchase document", () => {
    const { root, db } = setup("rentemester-payables-dup-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-dup-inbox-"));
    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-1001", 1250, 250);
    const first = registerPayable(db, { documentId, billDate: "2026-01-10", dueDate: "2026-02-09", expenseAccountNo: "3000" });
    expect(first.ok).toBe(true);
    const second = registerPayable(db, { documentId, billDate: "2026-01-10", dueDate: "2026-02-09", expenseAccountNo: "3000" });
    expect(second.ok).toBe(false);
    expect(second.errors.join(" ")).toContain("already registered");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("builds a kreditorliste with forfaldne / ikke-forfaldne aging buckets", () => {
    const { root, db } = setup("rentemester-payables-list-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-list-inbox-"));

    // Overdue bill: due 2026-02-09, as-of 2026-03-15 => 34 days overdue.
    const overdueDoc = ingestPurchase(db, root, inbox, "Software ApS", "V-1001", 1250, 250);
    registerPayable(db, { documentId: overdueDoc, billDate: "2026-01-10", dueDate: "2026-02-09", expenseAccountNo: "3000" });

    // Not-yet-due bill: due 2026-04-01, as-of 2026-03-15 => not overdue.
    const futureDoc = ingestPurchase(db, root, inbox, "Hosting ApS", "V-1002", 500, 100, "2026-03-02");
    registerPayable(db, { documentId: futureDoc, billDate: "2026-03-02", dueDate: "2026-04-01", expenseAccountNo: "3000" });

    const list = buildPayablesList(db, { asOfDate: "2026-03-15" });
    expect(list.ok).toBe(true);
    expect(list.count).toBe(2);
    expect(list.totalOpenBalance).toBe(1750);
    expect(list.overdueOpenBalance).toBe(1250);
    expect(list.notYetDueOpenBalance).toBe(500);

    const overdue = list.rows.find((r) => r.billNo === "V-1001")!;
    expect(overdue.isOverdue).toBe(true);
    expect(overdue.overdueDays).toBe(34);
    expect(overdue.agingBucket).toBe("31-60");

    const future = list.rows.find((r) => r.billNo === "V-1002")!;
    expect(future.isOverdue).toBe(false);
    expect(future.overdueDays).toBe(0);
    expect(future.agingBucket).toBe("not-due");

    // status filter narrows to open items only.
    const onlyOverdue = buildPayablesList(db, { asOfDate: "2026-03-15", status: "overdue" });
    expect(onlyOverdue.count).toBe(1);
    expect(onlyOverdue.rows[0]!.billNo).toBe("V-1001");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("defaulter asOfDate til i dag når den udelades — så overdue ikke skjules", () => {
    const { root, db } = setup("rentemester-payables-default-asof-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-default-asof-inbox-"));

    // Bill due far in the past — should be overdue against today's date.
    const overdueDoc = ingestPurchase(db, root, inbox, "Software ApS", "V-2001", 1250, 250, "2024-01-10");
    registerPayable(db, { documentId: overdueDoc, billDate: "2024-01-10", dueDate: "2024-02-09", expenseAccountNo: "3000" });

    // Must match how buildPayablesList computes its default — todayIsoDate()
    // returns the calendar date in Danish time (Europe/Copenhagen), whereas
    // new Date().toISOString() returns UTC. The two diverge for the ~1–2 hour
    // window each day between Danish midnight and UTC midnight, which would
    // otherwise make this test fail deterministically every night.
    const today = todayIsoDate();

    const list = buildPayablesList(db);
    expect(list.ok).toBe(true);
    expect(list.asOfDate).toBe(today);
    expect(list.count).toBe(1);
    const row = list.rows[0]!;
    expect(row.isOverdue).toBe(true);
    expect(row.overdueDays).toBeGreaterThan(0);
    expect(list.overdueOpenBalance).toBe(1250);

    // status: "overdue" without asOfDate must return the past-due bill, not an empty list.
    const overdue = buildPayablesList(db, { status: "overdue" });
    expect(overdue.ok).toBe(true);
    expect(overdue.count).toBe(1);
    expect(overdue.rows[0]!.billNo).toBe("V-2001");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  // §37: a NOT VAT-registered company cannot deduct input VAT, so a VAT-bearing
  // payable is absorbed (non_deductible) — the whole brutto hits the expense,
  // no 4000 line — and an explicit 'standard' is refused.
  function markNotRegistered(db: ReturnType<typeof openDb>) {
    db.run(
      "INSERT INTO companies (id, name, vat_period_type) VALUES (1, 'TEST HOLDING ApS', NULL) " +
        "ON CONFLICT(id) DO UPDATE SET vat_period_type = NULL",
    );
  }

  test("non-registered company: a VAT-bearing payable defaults to non_deductible (gross to expense, no 4000)", () => {
    const { root, db } = setup("rentemester-payables-nonreg-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-nonreg-inbox-"));
    markNotRegistered(db);
    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-ND-1", 1250, 250);

    const registered = registerPayable(db, {
      documentId,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccountNo: "3000",
    });
    expect({ ok: registered.ok, errors: registered.errors }).toEqual({ ok: true, errors: [] });

    const lines = db
      .query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
         WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
      )
      .all(registered.entryId!) as any[];
    // Gross to the expense, no 4000 Købsmoms line, no vat_code: never feeds a
    // momsangivelse rubrik.
    expect(lines).toEqual([
      { account_no: "3000", debit_amount: 1250, credit_amount: 0, vat_code: null },
      { account_no: "7000", debit_amount: 0, credit_amount: 1250, vat_code: null },
    ]);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("non-registered company: explicit vatTreatment='standard' payable is refused (points at non_deductible)", () => {
    const { root, db } = setup("rentemester-payables-nonreg-std-");
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-payables-nonreg-std-inbox-"));
    markNotRegistered(db);
    const documentId = ingestPurchase(db, root, inbox, "Software ApS", "V-ND-2", 1250, 250);

    const registered = registerPayable(db, {
      documentId,
      billDate: "2026-01-10",
      dueDate: "2026-02-09",
      expenseAccountNo: "3000",
      vatTreatment: "standard",
    });
    expect(registered.ok).toBe(false);
    expect(registered.errors.join(" ")).toContain("ikke momsregistreret");
    expect(registered.errors.join(" ")).toContain("non_deductible");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});
