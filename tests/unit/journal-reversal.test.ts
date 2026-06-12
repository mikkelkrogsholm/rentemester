// Tests: src/core/ledger.ts (journal entry reversal)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, reverseJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";

describe("journal reversal", () => {
  test("can reverse an imported-historical income/expense voucher that has no document", () => {
    // A replayed migration posting (#195) may carry income/expense lines and no
    // Rentemester document yet (#196 attaches the bilag later). Reversing it
    // must succeed AND keep the audit chain valid — the reversal inherits the
    // import exemption so it is not rejected for missing document evidence.
    const root = mkdtempSync(join(tmpdir(), "rentemester-reversal-import-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Imported historical expense",
      createdByProgram: "rentemester-import-postings",
      importedHistorical: true,
      lines: [
        { accountNo: "3000", debitAmount: 100, text: "Historisk udgift" },
        { accountNo: "2000", creditAmount: 100, text: "Bank" },
      ],
    });
    expect(posted.ok).toBe(true);

    const reversed = reverseJournalEntry(db, {
      entryId: posted.entryId!,
      transactionDate: "2026-05-17",
      reason: "Forkert migreringspostering",
    });
    expect(reversed.errors).toEqual([]);
    expect(reversed.ok).toBe(true);

    // The whole ledger — original imported voucher + its reversal — still
    // verifies clean (neither is flagged for missing document evidence).
    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("creates one linked reversal entry with mirrored lines and blocks a second reversal", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reversal-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-reversal-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Vendor invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-REV-1",
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
      text: "Original software booking",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25", text: "Software" },
        { accountNo: "4000", debitAmount: 250, text: "Input VAT" },
        { accountNo: "2000", creditAmount: 1250, text: "Bank" }
      ]
    });
    expect(posted.ok).toBe(true);

    const reversed = reverseJournalEntry(db, {
      entryId: posted.entryId!,
      transactionDate: "2026-05-17",
      reason: "Wrong booking period"
    });

    expect(reversed.ok).toBe(true);
    expect(reversed.originalEntryId).toBe(posted.entryId);
    expect(reversed.appliedRules).toContain("DK-BOOKKEEPING-REVERSAL-001");

    const reversalEntry = db.query("SELECT status, reversal_of_entry_id, text FROM journal_entries WHERE id = ?").get(reversed.entryId!) as any;
    expect(reversalEntry.status).toBe("reversed");
    expect(reversalEntry.reversal_of_entry_id).toBe(posted.entryId);
    expect(reversalEntry.text).toContain("Reversal of");

    const originalLines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    const reversalLines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(reversed.entryId!) as any[];

    expect(reversalLines).toHaveLength(originalLines.length);
    for (let i = 0; i < originalLines.length; i++) {
      expect(reversalLines[i].account_no).toBe(originalLines[i].account_no);
      expect(reversalLines[i].debit_amount).toBe(originalLines[i].credit_amount);
      expect(reversalLines[i].credit_amount).toBe(originalLines[i].debit_amount);
    }

    const second = reverseJournalEntry(db, {
      entryId: posted.entryId!,
      transactionDate: "2026-05-18",
      reason: "Try again"
    });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("already has reversal");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);
    expect(chain.entries).toBe(2);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});
