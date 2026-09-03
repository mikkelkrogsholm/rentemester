import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { addBankAccount, importBankCsv } from "../../src/core/bank";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { postJournalEntry } from "../../src/core/ledger";
import {
  applyImportedReceivableBankSettlement,
  getImportedReceivableBankSettlement,
  listImportedReceivables,
  planImportedReceivableBankSettlement,
  recordImportedReceivableSchedule,
} from "../../src/core/imported-receivables";

const hash = (letter: string) => letter.repeat(64);

function fixture(amount = 100) {
  const root = mkdtempSync(join(tmpdir(), "rentemester-imported-receivable-settlement-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.query("INSERT INTO dinero_import_sources(id,raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count) VALUES(1,?,1,?,0)").run(hash("a"), hash("b"));
  db.query("INSERT INTO dinero_import_inventories(id,source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes) VALUES(1,1,?,?,0,0,0)").run(hash("a"), hash("b"));
  db.query("INSERT INTO dinero_import_attempts(id,inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256) VALUES(1,1,1,?,'dinero-v4','agent:test','2025-01-01','accepted',?)").run(hash("a"), hash("c"));
  expect(recordImportedReceivableSchedule(db, 1, {
    contract: "rentemester-imported-receivables-v1",
    sourceDocumentHash: hash("d"),
    invoices: [{ id: "LEGACY-1", invoiceDate: "2025-01-01", grossAmount: amount, controlAccountNo: "1100", recognitionRef: "legacy:1", documentHash: hash("e"), payments: [] }],
  }, "2025-01-31").ok).toBe(true);
  const bankAccount = addBankAccount(db, { slug: "synthetic-bank", name: "Synthetic bank", currency: "DKK", ledgerAccountNo: "2000" });
  if (!bankAccount.ok) throw new Error(bankAccount.errors.join("; "));
  return { root, db, bankAccountId: bankAccount.account!.id, cleanup() { db.close(); rmSync(root, { recursive: true, force: true }); } };
}

function bankTx(f: ReturnType<typeof fixture>, amount: number, reference: string, date = "2025-02-01") {
  const path = join(f.root, `${reference}.csv`);
  writeFileSync(path, `transaction_date,booking_date,text,amount,currency,reference\n${date},${date},Synthetic receipt,${amount},DKK,${reference}\n`);
  expect(importBankCsv(f.db, f.root, path, { account: f.bankAccountId }).ok).toBe(true);
  return f.db.query("SELECT id FROM bank_transactions WHERE reference=?").get(reference) as { id: number };
}

function planInput(scheduleHash: string, bankTransactionId: number) {
  return { scheduleHash, externalInvoiceId: "LEGACY-1", bankTransactionId };
}

describe("imported receivable bank settlement v44", () => {
  test("plans read-only and atomically settles an exact imported receivable", () => {
    const f = fixture();
    const before = f.db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
    const scheduleHash = (listImportedReceivables(f.db, "2025-02-01").rows[0] as any).scheduleHash;
    const tx = bankTx(f, 100, "RECEIPT-1");
    const plan = planImportedReceivableBankSettlement(f.db, planInput(scheduleHash, tx.id));
    expect(plan).toMatchObject({ ok: true, plan: { openAmount: 100, amount: 100 } });
    expect(f.db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual(before);
    if (!plan.ok) throw new Error("fixture");
    const applied = applyImportedReceivableBankSettlement(f.db, { ...planInput(scheduleHash, tx.id), planHash: plan.plan.planHash, idempotencyKey: "exact-1", actor: "agent:test", principal: { kind: "service-account", subjectId: "svc:test" }, confirm: true });
    expect(applied).toMatchObject({ ok: true, idempotent: false, openBalance: 0, journalEntryId: expect.any(Number) });
    expect(listImportedReceivables(f.db, "2025-02-01").rows[0]).toMatchObject({ openBalance: 0, paidAmount: 100 });
    expect(getImportedReceivableBankSettlement(f.db, tx.id)).toMatchObject({ ok: true, settlement: { bank_transaction_id: tx.id, external_invoice_id: "LEGACY-1" } });
    expect(verifyAuditChain(f.db).ok).toBe(true);
    f.cleanup();
  });

  test("supports a partial receipt while preserving the remaining canonical balance", () => {
    const f = fixture();
    const scheduleHash = (listImportedReceivables(f.db, "2025-02-01").rows[0] as any).scheduleHash;
    const tx = bankTx(f, 40, "RECEIPT-PARTIAL");
    const plan = planImportedReceivableBankSettlement(f.db, planInput(scheduleHash, tx.id));
    if (!plan.ok) throw new Error(plan.errors.join("; "));
    expect(applyImportedReceivableBankSettlement(f.db, { ...planInput(scheduleHash, tx.id), planHash: plan.plan.planHash, idempotencyKey: "partial-1", actor: "agent:test", principal: { kind: "service-account", subjectId: "svc:test" }, confirm: true })).toMatchObject({ ok: true, openBalance: 60 });
    expect(listImportedReceivables(f.db, "2025-02-01").rows[0]).toMatchObject({ paidAmount: 40, openBalance: 60 });
    f.cleanup();
  });

  test("is idempotent and rejects stale plans, overpayment and reconciled bank transactions", () => {
    const f = fixture();
    const scheduleHash = (listImportedReceivables(f.db, "2025-02-01").rows[0] as any).scheduleHash;
    const tx = bankTx(f, 100, "RECEIPT-RETRY");
    const input = planInput(scheduleHash, tx.id);
    const plan = planImportedReceivableBankSettlement(f.db, input); if (!plan.ok) throw new Error("fixture");
    const apply = { ...input, planHash: plan.plan.planHash, idempotencyKey: "retry-1", actor: "agent:test", principal: { kind: "service-account" as const, subjectId: "svc:test" }, confirm: true };
    expect(applyImportedReceivableBankSettlement(f.db, apply)).toMatchObject({ ok: true, idempotent: false });
    expect(applyImportedReceivableBankSettlement(f.db, apply)).toMatchObject({ ok: true, idempotent: true });
    expect(applyImportedReceivableBankSettlement(f.db, { ...apply, planHash: hash("0") })).toMatchObject({ ok: false, errors: ["IDEMPOTENCY_CONFLICT"] });
    const count = f.db.query("SELECT COUNT(*) AS n FROM imported_receivable_bank_settlements").get(); expect(count).toEqual({ n: 1 });
    f.cleanup();

    const overpaid = fixture(); const overSchedule = (listImportedReceivables(overpaid.db, "2025-02-01").rows[0] as any).scheduleHash; const overTx = bankTx(overpaid, 101, "RECEIPT-OVER");
    expect(planImportedReceivableBankSettlement(overpaid.db, planInput(overSchedule, overTx.id))).toMatchObject({ ok: false, errors: expect.arrayContaining(["IMPORTED_RECEIVABLE_OVERPAYMENT"]) });
    overpaid.cleanup();

    const reconciled = fixture(); const reconciledSchedule = (listImportedReceivables(reconciled.db, "2025-02-01").rows[0] as any).scheduleHash; const reconciledTx = bankTx(reconciled, 100, "RECEIPT-RECONCILED");
    expect(postJournalEntry(reconciled.db, { transactionDate: "2025-02-01", text: "Synthetic prior reconciliation", sourceBankTransactionId: reconciledTx.id, createdBy: "agent:test", lines: [{ accountNo: "2000", debitAmount: 100 }, { accountNo: "5000", creditAmount: 100 }] }).ok).toBe(true);
    expect(planImportedReceivableBankSettlement(reconciled.db, planInput(reconciledSchedule, reconciledTx.id))).toMatchObject({ ok: false, errors: expect.arrayContaining(["BANK_TRANSACTION_ALREADY_RECONCILED"]) });
    reconciled.cleanup();
  });
});
