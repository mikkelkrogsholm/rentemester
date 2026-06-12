// Tests: src/core/ledger.ts (foreign-currency posting, actor attribution, period locks, document evidence)
// Companion of journal-post.test.ts and ledger-hardening.test.ts.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { postJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { closeAccountingPeriod } from "../../src/core/periods";

import { cleanupDir } from "../helpers/cleanup";
describe("journal posting — FX, attribution & period locks", () => {
  test("supports foreign-currency journal entries with stored FX basis", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-fx-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-inbox-fx-"));
    const sourceFile = join(inbox, "vendor-eur.txt");
    writeFileSync(sourceFile, "Vendor invoice\n100 EUR\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-19",
      invoiceNo: "INV-EUR-1",
      deliveryDescription: "Softwareabonnement EUR",
      amountIncVat: 746,
      currency: "DKK",
      sender: { name: "Leverandør GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 149.2,
      paymentDetails: "Kortbetaling"
    });
    expect(doc.ok).toBe(true);

    const badFx = postJournalEntry(db, {
      transactionDate: "2026-05-19",
      text: "FX journal without conversion basis",
      documentId: doc.documentId,
      currency: "EUR",
      lines: [
        { accountNo: "3000", debitAmount: 596.8, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 149.2 },
        { accountNo: "2000", creditAmount: 746 }
      ]
    });
    expect(badFx.ok).toBe(false);
    expect(badFx.errors).toContain("amountForeign must be positive for non-DKK journal entries");

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-19",
      text: "FX journal with conversion basis",
      documentId: doc.documentId,
      currency: "EUR",
      amountForeign: 100,
      amountDkk: 746,
      fxRateToDkk: 7.46,
      lines: [
        { accountNo: "3000", debitAmount: 596.8, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 149.2 },
        { accountNo: "2000", creditAmount: 746 }
      ]
    });

    expect(posted.ok).toBe(true);
    expect(posted.appliedRules).toContain("DK-BOOKKEEPING-FX-001");
    const entry = db.query("SELECT currency, amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(posted.entryId!) as any;
    expect(entry).toEqual({ currency: "EUR", amount_foreign: 100, amount_dkk: 746, fx_rate_to_dkk: 7.46 });

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("normalizes FX payloads before persistence so audit verification reads the same rounded values", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-fx-normalized-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-journal-fx-normalized-inbox-"));
    const sourceFile = join(inbox, "invoice.txt");
    writeFileSync(sourceFile, "Invoice\n745.56 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-19",
      invoiceNo: "INV-EUR-2",
      deliveryDescription: "Softwareabonnement EUR",
      amountIncVat: 745.56,
      currency: "DKK",
      sender: { name: "Leverandør GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 149.11,
      paymentDetails: "Kortbetaling"
    });
    expect(doc.ok).toBe(true);

    const posted = postJournalEntry(db, {
      transactionDate: "2026-05-19",
      text: "FX journal with normalized conversion basis",
      documentId: doc.documentId,
      currency: "EUR",
      amountForeign: 100,
      amountDkk: 745.56,
      fxRateToDkk: 7.4555555,
      lines: [
        { accountNo: "3000", debitAmount: 596.45, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 149.11 },
        { accountNo: "2000", creditAmount: 745.56 }
      ]
    });

    expect(posted.ok).toBe(true);
    const entry = db.query("SELECT amount_foreign, amount_dkk, fx_rate_to_dkk FROM journal_entries WHERE id = ?").get(posted.entryId!) as any;
    expect(entry).toEqual({ amount_foreign: 100, amount_dkk: 745.56, fx_rate_to_dkk: 7.455556 });
    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("records actor attribution from environment for direct journal posts", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-actor-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const prevActor = process.env.RENTEMESTER_ACTOR;
    const prevVia = process.env.RENTEMESTER_ACTOR_VIA;
    process.env.RENTEMESTER_ACTOR = "agent:freja";
    process.env.RENTEMESTER_ACTOR_VIA = "openclaw";

    try {
      const posted = postJournalEntry(db, {
        transactionDate: "2026-05-16",
        text: "Owner contribution",
        lines: [
          { accountNo: "2000", debitAmount: 1000 },
          { accountNo: "5000", creditAmount: 1000 }
        ]
      });

      expect(posted.ok).toBe(true);
      const entry = db.query("SELECT created_by, created_by_program FROM journal_entries WHERE id = ?").get(posted.entryId!) as any;
      expect(entry).toEqual({ created_by: "agent:freja", created_by_program: "openclaw" });
      const audit = db.query("SELECT actor FROM audit_log WHERE event_type = 'journal_post' ORDER BY id DESC LIMIT 1").get() as any;
      expect(audit.actor).toBe("agent:freja via openclaw");
    } finally {
      if (prevActor === undefined) delete process.env.RENTEMESTER_ACTOR; else process.env.RENTEMESTER_ACTOR = prevActor;
      if (prevVia === undefined) delete process.env.RENTEMESTER_ACTOR_VIA; else process.env.RENTEMESTER_ACTOR_VIA = prevVia;
      db.close();
      cleanupDir(root);
    }
  });

  test("blocks closed-period and future-dated journal postings while allowing correcting entries in an open period", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-period-lock-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-05-01",
      periodEnd: "2026-05-31",
      kind: "vat_quarter",
      reference: "SKAT-Q2-2026"
    });
    expect(closed.ok).toBe(true);

    const insideClosed = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Late backpost into closed period",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(insideClosed.ok).toBe(false);
    expect(insideClosed.errors).toContain("transactionDate 2026-05-16 falls in closed period vat_quarter 2026-05-01..2026-05-31 ref SKAT-Q2-2026");

    const futureDated = postJournalEntry(db, {
      transactionDate: "2099-12-31",
      text: "Future-dated hiding entry",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(futureDated.ok).toBe(false);
    expect(futureDated.errors.some((error) => error.includes("transactionDate 2099-12-31 cannot be later than"))).toBe(true);

    const correction = postJournalEntry(db, {
      transactionDate: "2026-06-01",
      text: "Correction posted in next open period",
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "5000", creditAmount: 1000 }
      ]
    });
    expect(correction.ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("requires document evidence for expense or income postings and hashes lines into the audit chain", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-inbox-"));
    const sourceFile = join(inbox, "vendor.txt");
    writeFileSync(sourceFile, "Vendor invoice\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const missingDoc = postJournalEntry(db, {
      transactionDate: "2026-05-16",
      text: "Software expense without evidence",
      lines: [
        { accountNo: "3000", debitAmount: 1000 },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 }
      ]
    });
    expect(missingDoc.ok).toBe(false);
    expect(missingDoc.errors).toContain("documentId is required when posting expense or income lines");

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-3000",
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
        { accountNo: "2000", creditAmount: 1250, text: "Bank payment" }
      ]
    });

    expect(posted.ok).toBe(true);
    expect(posted.entryNo).toBeDefined();
    expect(posted.appliedRules).toContain("DK-BOOKKEEPING-DOCUMENT-001");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);
    expect(chain.entries).toBe(1);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    expect(lines).toHaveLength(3);
    expect(lines[0].vat_code).toBe("DK_PURCHASE_25");

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
