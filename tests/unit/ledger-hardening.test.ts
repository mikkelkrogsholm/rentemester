// Tests: src/core/ledger.ts (append-only triggers, mutation protection, audit-chain corruption detection)
// Companion of journal-post.test.ts and journal-post-fx.test.ts.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { closeAccountingPeriod } from "../../src/core/periods";

import { cleanupDir } from "../helpers/cleanup";
describe("ledger hardening", () => {
  test("prevents direct mutation of journal lines after posting", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-ledger-lines-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Owner contribution",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);

    expect(() => db.run("UPDATE journal_lines SET debit_amount = 999 WHERE journal_entry_id = ?", posted.entryId!)).toThrow("journal_lines are append-only");
    expect(() => db.run("DELETE FROM journal_lines WHERE journal_entry_id = ?", posted.entryId!)).toThrow("journal_lines are append-only");

    db.close();
    cleanupDir(root);
  });

  test("prevents mutation or deletion of purchase documents once linked to a journal entry", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-linked-doc-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-linked-doc-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Vendor invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-LINKED",
      deliveryDescription: "Softwareabonnement",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bankoverførsel"
    });
    expect(doc.ok).toBe(true);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Software expense with evidence",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 }
      ]
    });
    expect(posted.ok).toBe(true);

    expect(() => db.run("UPDATE documents SET amount_inc_vat = 999 WHERE id = ?", doc.documentId!)).toThrow("document is linked to a journal entry");
    expect(() => db.run("DELETE FROM documents WHERE id = ?", doc.documentId!)).toThrow("document is linked to a journal entry");

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("enforces one posted journal entry per source bank transaction at database level", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-unique-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = db.query(
      `INSERT INTO bank_transactions (transaction_date, text, amount, transaction_hash)
       VALUES ('2026-05-16', 'Customer payment', 1000, 'unique-bank-source-test')
       RETURNING id`
    ).get() as { id: number };

    const first = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Bank-linked posting",
      sourceBankTransactionId: bank.id,
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(first.ok).toBe(true);

    expect(() => postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Duplicate bank-linked posting",
      sourceBankTransactionId: bank.id,
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    })).toThrow("UNIQUE constraint failed");

    db.close();
    cleanupDir(root);
  });

  test("prevents mutation or deletion of referenced bank transactions", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bank-append-only-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const bank = db.query(
      `INSERT INTO bank_transactions (transaction_date, text, amount, transaction_hash)
       VALUES ('2026-05-16', 'Customer payment', 1000, 'bank-append-only-test')
       RETURNING id`
    ).get() as { id: number };

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Bank-linked posting",
      sourceBankTransactionId: bank.id,
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);

    expect(() => db.run("UPDATE bank_transactions SET amount = 9999 WHERE id = ?", bank.id)).toThrow("bank transaction is referenced by ledger or payment records and cannot be modified");
    expect(() => db.run("DELETE FROM bank_transactions WHERE id = ?", bank.id)).toThrow("bank transactions are append-only");

    db.close();
    cleanupDir(root);
  });

  test("protects compliance tables against destructive rewrites", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-compliance-append-only-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Compliance hardening proof",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);

    const audit = db.query("SELECT id FROM audit_log WHERE event_type = 'journal_post' ORDER BY id DESC LIMIT 1").get() as { id: number };
    expect(() => db.run("UPDATE audit_log SET actor = 'spoof@example.com' WHERE id = ?", audit.id)).toThrow("audit_log is append-only");
    expect(() => db.run("DELETE FROM audit_log WHERE id = ?", audit.id)).toThrow("audit_log is append-only");

    const period = closeAccountingPeriod(db, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      kind: "custom",
      status: "closed"
    });
    expect(period.ok).toBe(true);
    expect(() => db.run("UPDATE accounting_periods SET status = 'open' WHERE id = ?", period.periodId!)).toThrow("accounting periods may only progress open -> closed -> reported; period bounds are immutable");
    expect(() => db.run("DELETE FROM accounting_periods WHERE id = ?", period.periodId!)).toThrow("accounting periods are append-only");

    expect(() => db.run("UPDATE sequences SET value = value - 1 WHERE kind = 'journal_entry'")).toThrow("sequences are immutable identifiers and monotonically increasing");
    expect(() => db.run("DELETE FROM sequences WHERE kind = 'journal_entry'")).toThrow("sequences are append-only");

    const exception = db.query(
      `INSERT INTO exceptions (type, severity, status, message, required_action)
       VALUES ('UNMATCHED_BANK_TRANSACTION', 'high', 'open', 'Needs review', 'Match to document')
       RETURNING id`
    ).get() as { id: number };
    db.run("UPDATE exceptions SET status = 'resolved', resolved_at = CURRENT_TIMESTAMP, resolved_by = 'tester', resolution_note = 'done' WHERE id = ?", exception.id);
    expect(() => db.run("UPDATE exceptions SET status = 'open' WHERE id = ?", exception.id)).toThrow("exceptions may only progress from open to resolved; identity is immutable");
    expect(() => db.run("DELETE FROM exceptions WHERE id = ?", exception.id)).toThrow("exceptions are append-only; resolve them instead");

    expect(() => db.run("UPDATE companies SET fiscal_year_start_month = 7 WHERE id = 1")).toThrow("fiscal year configuration is locked after the first journal entry");

    db.close();
    cleanupDir(root);
  });

  test("restores dropped and tampered append-only triggers when migrate runs again", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-trigger-restore-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    // A dropped trigger leaves the table unprotected.
    db.run("DROP TRIGGER journal_entries_no_delete");
    expect(db.query("SELECT name FROM sqlite_master WHERE type = 'trigger' AND name = 'journal_entries_no_delete'").get()).toBeNull();

    // A tampered trigger still exists by name but has a harmless no-op body,
    // so CREATE TRIGGER IF NOT EXISTS would silently leave it broken.
    db.run("DROP TRIGGER journal_lines_no_delete");
    db.run("CREATE TRIGGER journal_lines_no_delete BEFORE DELETE ON journal_lines BEGIN SELECT 1; END;");

    migrate(db);

    const restored = db.query("SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'journal_entries_no_delete'").get() as { sql: string } | null;
    expect(restored).not.toBeNull();
    expect(restored!.sql).toContain("append-only");
    const repaired = db.query("SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'journal_lines_no_delete'").get() as { sql: string };
    expect(repaired.sql).toContain("append-only");

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Entry protected by restored trigger",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(posted.ok).toBe(true);
    expect(() => db.run("DELETE FROM journal_entries WHERE id = ?", posted.entryId!)).toThrow("journal_entries are append-only");
    expect(() => db.run("DELETE FROM journal_lines WHERE journal_entry_id = ?", posted.entryId!)).toThrow("journal_lines are append-only");

    db.close();
    cleanupDir(root);
  });

  test("audit verify detects structural ledger corruption beyond hash mismatch", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-audit-corrupt-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    db.exec("PRAGMA foreign_keys = OFF");
    db.run(
      `INSERT INTO journal_entries (entry_no, transaction_date, text, rule_version, status, previous_hash, entry_hash)
       VALUES ('2026-99999', '2026-05-16', 'Corrupt entry without lines', 'corrupt-fixture', 'posted', 'GENESIS', 'bad-hash')`
    );
    db.run(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount, text)
       VALUES (999999, 999999, 1, 0, 'orphan broken account')`
    );
    db.exec("PRAGMA foreign_keys = ON");

    const result = verifyAuditChain(db);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("entry has no journal lines"))).toBe(true);
    expect(result.errors.some((e) => e.includes("orphan journal_entry_id"))).toBe(true);
    expect(result.errors.some((e) => e.includes("missing account_id"))).toBe(true);
    expect(result.errors.some((e) => e.includes("foreign key violation"))).toBe(true);

    db.close();
    cleanupDir(root);
  });
  test("audit verify cross-checks stored invoice status against ledger balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-audit-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-00001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    const posted = postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);

    db.run("DROP TRIGGER documents_no_update_issued_invoice");
    db.run("UPDATE documents SET status = 'paid' WHERE id = ?", issued.documentId!);

    const result = verifyAuditChain(db);
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => e.includes("stored status paid does not match ledger status open"))).toBe(true);

    db.close();
    cleanupDir(root);
  });

});
