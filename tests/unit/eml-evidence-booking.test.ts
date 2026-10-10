// Tests: src/core/expense-booking.ts + src/core/ledger.ts dry-run against
// attachmentless EML evidence documents (#566 — booking-controls eligibility).
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, dryRunJournalEntry, verifyAuditChain } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { bookExpenseFromBank, previewBookExpenseFromBank } from "../../src/core/expense-booking";

const EML = [
  "From: bog@leverandoer.dk",
  "To: konto@firma.dk",
  "Subject: Ordrebekraeftigelse ORD-99101",
  "Date: Sat, 16 May 2026 10:00:00 +0200",
  "Message-ID: <ord-99101@leverandoer.dk>",
  "MIME-Version: 1.0",
  "Content-Type: text/plain; charset=utf-8",
  "",
  "Ordre ORD-99101, i alt 1250,00 DKK incl. 250,00 moms.",
  "",
].join("\r\n");

function fixture() {
  const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-emlbook-"));
  const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-emlbookdrop-"));
  const emlFile = join(dropRoot, "receipt.eml");
  writeFileSync(emlFile, EML);
  const db = openDb(ensureCompanyDirs(companyRoot).db);
  migrate(db);
  seedAccounts(db);
  const ingest = ingestDocument(db, companyRoot, emlFile, {
    source: "email",
    issueDate: "2026-05-16",
    invoiceNo: "ORD-99101",
    deliveryDescription: "Kontorartikler",
    amountIncVat: 1250,
    currency: "DKK",
    sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 250,
  });
  if (!ingest.ok || ingest.documentId == null) throw new Error(`EML ingest failed: ${ingest.errors?.join("; ")}`);
  db.run(
    "INSERT INTO bank_transactions (transaction_date, text, amount, currency, transaction_hash, status) VALUES ('2026-05-18', 'Ordre 99101', -1250, 'DKK', 'hash-emlbook-1', 'imported')",
  );
  const bankId = (db.query("SELECT id FROM bank_transactions WHERE transaction_hash = 'hash-emlbook-1'").get() as { id: number }).id;
  if (!(bankId > 0)) throw new Error("bank fixture insert failed");
  const row = db.query("SELECT stored_path FROM documents WHERE id = ?").get(ingest.documentId!) as { stored_path: string };
  return { db, companyRoot, dropRoot, documentId: ingest.documentId!, bankId, storedPath: row.stored_path };
}

describe("#566 — EML evidence document is eligible for booking controls", () => {
  test("expense preview (dry-run control) accepts the EML evidence document", () => {
    const fx = fixture();
    try {
      const preview = previewBookExpenseFromBank(fx.db, {
        documentId: fx.documentId,
        bankTransactionId: fx.bankId,
        expenseAccountNo: "3000",
        vatTreatment: "standard",
      });
      if (!preview.ok) throw new Error(preview.errors.join("; "));
      expect(preview.ok).toBe(true);
    } finally {
      fx.db.close();
      rmSync(fx.companyRoot, { recursive: true, force: true });
      rmSync(fx.dropRoot, { recursive: true, force: true });
    }
  });

  test("dryRunJournalEntry accepts a plain journal dry-run referencing the EML evidence", () => {
    const fx = fixture();
    try {
      const dryRun = dryRunJournalEntry(fx.db, {
        transactionDate: "2026-05-18",
        text: "Udgift — ordre 99101",
        documentId: fx.documentId,
        lines: [
          { accountNo: "3000", debitAmount: 1250 },
          { accountNo: "2000", creditAmount: 1250 },
        ],
      });
      if (!dryRun.ok) throw new Error(dryRun.errors.join("; "));
      expect(dryRun.ok).toBe(true);
    } finally {
      fx.db.close();
      rmSync(fx.companyRoot, { recursive: true, force: true });
      rmSync(fx.dropRoot, { recursive: true, force: true });
    }
  });

  test("books the expense and the audit verify passes with the EML evidence attached", () => {
    const fx = fixture();
    try {
      const booked = bookExpenseFromBank(fx.db, {
        documentId: fx.documentId,
        bankTransactionId: fx.bankId,
        expenseAccountNo: "3000",
        vatTreatment: "standard",
      });
      if (!booked.ok) throw new Error(booked.errors.join("; "));
      const lines = fx.db.query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
           FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
          WHERE jl.journal_entry_id = ? ORDER BY jl.id`).all(booked.entryId!) as any[];
      expect(lines).toEqual([
        { account_no: "3000", debit_amount: 1000, credit_amount: 0, vat_code: "DK_PURCHASE_25" },
        { account_no: "4000", debit_amount: 250, credit_amount: 0, vat_code: null },
        { account_no: "2000", debit_amount: 0, credit_amount: 1250, vat_code: null },
      ]);
      const verify = verifyAuditChain(fx.db, { companyRoot: fx.companyRoot });
      expect(verify.ok).toBe(true);
    } finally {
      fx.db.close();
      rmSync(fx.companyRoot, { recursive: true, force: true });
      rmSync(fx.dropRoot, { recursive: true, force: true });
    }
  });

  test("fail-closed: tampering the stored .eml evidence after booking is flagged by audit verify", () => {
    const fx = fixture();
    try {
      const booked = bookExpenseFromBank(fx.db, {
        documentId: fx.documentId,
        bankTransactionId: fx.bankId,
        expenseAccountNo: "3000",
        vatTreatment: "standard",
      });
      if (!booked.ok) throw new Error(booked.errors.join("; "));

      const originalBytes = readFileSync(fx.storedPath);
      writeFileSync(fx.storedPath, originalBytes.toString("utf8").replace("1250,00 DKK", "990,00 DKK"));

      const verify = verifyAuditChain(fx.db, { companyRoot: fx.companyRoot });
      expect(verify.ok).toBe(false);
      expect(verify.errors.join(" ")).toContain("stored evidence sha256 does not match the document register");
    } finally {
      fx.db.close();
      rmSync(fx.companyRoot, { recursive: true, force: true });
      rmSync(fx.dropRoot, { recursive: true, force: true });
    }
  });
});
