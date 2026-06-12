// Tests: src/core/expense-booking.ts + src/agent/loop.ts — issue #302.
//
// English text must not leak onto Danish-facing surfaces. A non-technical
// Danish owner reads the posting entry text and the agent exception queue;
// both must be fully Danish.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { cleanupDir } from "../helpers/cleanup";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { bookExpenseFromBank } from "../../src/core/expense-booking";
import { initialiseCompanyVolume } from "../../src/core/company";
import { runAgentLoop } from "../../src/agent/loop";

const DEMO_DIR = join(import.meta.dir, "..", "..", "examples", "agent-demo");

describe("#302 — expense posting text is fully Danish", () => {
  test("the default posting text and the per-line fallback are Danish, not English", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-302-expense-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-302-expense-inbox-"));
    try {
      const csv = join(root, "transactions.csv");
      const sourceFile = join(inbox, "vendor.txt");
      writeFileSync(csv, [
        "transaction_date,booking_date,text,amount,currency,reference",
        "2026-05-16,2026-05-16,LEVERANDOER APS,-1250,DKK,REF-302-1",
      ].join("\n"));
      writeFileSync(sourceFile, "Faktura\n1250 DKK\n");

      const db = openDb(ensureCompanyDirs(root).db);
      migrate(db);
      seedAccounts(db);

      const bank = importBankCsv(db, root, csv);
      expect(bank.ok).toBe(true);

      // No invoiceNo — forces the per-line fallback text path too.
      const doc = ingestDocument(db, root, sourceFile, {
        source: "email",
        issueDate: "2026-05-16",
        deliveryDescription: "Softwareabonnement",
        amountIncVat: 1250,
        currency: "DKK",
        sender: { name: "Leverandoer ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
        recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        vatAmount: 250,
        paymentDetails: "Bank transfer",
      });
      expect(doc.ok).toBe(true);

      const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-302-1'").get() as { id: number };
      const booked = bookExpenseFromBank(db, {
        documentId: doc.documentId!,
        bankTransactionId: bankRow.id,
        expenseAccountNo: "3000",
      });
      expect(booked.ok).toBe(true);

      const entry = db.query("SELECT text FROM journal_entries WHERE id = ?").get(booked.entryId!) as { text: string };
      // The posting text must be fully Danish — no English "from bank
      // transaction" leak, and the fallback word must be Danish "Udgift".
      expect(entry.text).not.toContain("from bank transaction");
      expect(entry.text).not.toContain("Expense");
      expect(entry.text).toContain("Udgift fra");
      expect(entry.text).toContain("banktransaktion");
      expect(entry.text).toContain("Leverandoer ApS");

      // The expense-base journal line falls back to a Danish word when there
      // is no invoice number.
      const lines = db.query(
        `SELECT a.account_no, jl.text
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
         WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
      ).all(booked.entryId!) as Array<{ account_no: string; text: string }>;
      const expenseLine = lines.find((l) => l.account_no === "3000")!;
      expect(expenseLine.text).not.toContain("Expense");
    } finally {
      cleanupDir(root, inbox);
    }
  });

  test("the no-supplier-name posting text falls back to a Danish word", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-302-expense-noname-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-302-expense-noname-inbox-"));
    try {
      const csv = join(root, "transactions.csv");
      const sourceFile = join(inbox, "vendor.txt");
      writeFileSync(csv, [
        "transaction_date,booking_date,text,amount,currency,reference",
        "2026-05-16,2026-05-16,KVITTERING,-1250,DKK,REF-302-NONAME",
      ].join("\n"));
      writeFileSync(sourceFile, "Kvittering\n1250 DKK\n");

      const db = openDb(ensureCompanyDirs(root).db);
      migrate(db);
      seedAccounts(db);

      const bank = importBankCsv(db, root, csv);
      expect(bank.ok).toBe(true);

      // A cash-register receipt may carry no sender name at all.
      const doc = ingestDocument(db, root, sourceFile, {
        source: "photo-upload",
        documentType: "cash_register_receipt",
        issueDate: "2026-05-16",
        amountIncVat: 1250,
        currency: "DKK",
        sender: { name: "Ukendt forretning" },
        vatAmount: 250,
        paymentDetails: "Card payment",
      });
      expect(doc.ok).toBe(true);

      const bankRow = db.query("SELECT id FROM bank_transactions WHERE reference = 'REF-302-NONAME'").get() as { id: number };
      const booked = bookExpenseFromBank(db, {
        documentId: doc.documentId!,
        bankTransactionId: bankRow.id,
        expenseAccountNo: "3000",
      });
      expect(booked.ok).toBe(true);

      const entry = db.query("SELECT text FROM journal_entries WHERE id = ?").get(booked.entryId!) as { text: string };
      expect(entry.text).not.toContain("Expense");
      expect(entry.text).not.toContain("from bank transaction");
      expect(entry.text).toContain("Udgift");
    } finally {
      cleanupDir(root, inbox);
    }
  });
});

describe("#302 — agent exception messages and the VAT-deadline note are Danish", () => {
  function freshCompany(): string {
    const root = mkdtempSync(join(tmpdir(), "rentemester-302-agent-"));
    initialiseCompanyVolume(root, { cvr: "DK12345678" });
    return root;
  }

  test("the open VAT-deadline exception is Danish, not English", () => {
    const root = freshCompany();
    try {
      // 2026-05-20: the previous VAT quarter (Q1 2026) is still open and its
      // momsangivelse deadline (2026-06-01) is within the escalation horizon,
      // so AGENT_VAT_DEADLINE_OPEN fires.
      const report = runAgentLoop({ companyRoot: root, asOf: "2026-05-20" });
      expect(report.ok).toBe(true);

      const vatEx = report.openExceptions.find((x) => x.type === "AGENT_VAT_DEADLINE_OPEN");
      expect(vatEx).toBeDefined();

      // No English wording leaks through.
      expect(vatEx!.message).not.toContain("VAT quarter");
      expect(vatEx!.message).not.toContain("is not closed");
      expect(vatEx!.message).not.toContain("is due");
      expect(vatEx!.message).not.toContain("days from");
      expect(vatEx!.requiredAction ?? "").not.toContain("Close the VAT period");
      expect(vatEx!.requiredAction ?? "").not.toContain("file the");

      // It is genuinely Danish and actionable.
      expect(vatEx!.message).toContain("Momskvartalet");
      expect(vatEx!.message.toLowerCase()).toContain("momsangivelse");
      expect(vatEx!.requiredAction ?? "").toContain("period close");
    } finally {
      cleanupDir(root);
    }
  });

  test("the open VAT-deadline note on the upcoming-deadline list stays Danish", () => {
    const root = freshCompany();
    try {
      const report = runAgentLoop({ companyRoot: root, asOf: "2026-05-20" });
      const openQuarter = report.upcomingDeadlines.find(
        (d) => d.kind === "vat_quarter" && !d.ready,
      );
      expect(openQuarter).toBeDefined();
      // The note (already Danish before #302) must not regress.
      expect(openQuarter!.note).not.toMatch(/\b(is|not|closed|the)\b/);
      expect(openQuarter!.note).toContain("Momsperioden");
    } finally {
      cleanupDir(root);
    }
  });

  test("the document-rejected exception is Danish, not English", () => {
    const root = freshCompany();
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-302-rejected-inbox-"));
    try {
      // A bilag whose metadata is missing a required field — the ledger
      // rejects it (a non-duplicate rejection), so AGENT_DOCUMENT_REJECTED
      // fires.
      const bilag = join(inbox, "ufuldstaendig.txt");
      writeFileSync(bilag, "Faktura\n1000 DKK\n");
      writeFileSync(join(inbox, "ufuldstaendig.json"), JSON.stringify({
        source: "email",
        issueDate: "2026-05-10",
        // deliveryDescription deliberately omitted -> rejected by the ledger.
        amountIncVat: 1000,
        currency: "DKK",
        sender: { name: "Test ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
        recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        vatAmount: 200,
        paymentDetails: "Bank transfer",
      }));

      const report = runAgentLoop({
        companyRoot: root,
        asOf: "2026-05-20",
        inboxDir: inbox,
      });
      expect(report.documentsRejected).toBe(1);

      const rejectedEx = report.openExceptions.find((x) => x.type === "AGENT_DOCUMENT_REJECTED");
      expect(rejectedEx).toBeDefined();
      expect(rejectedEx!.message).not.toContain("was rejected by the ledger");
      expect(rejectedEx!.requiredAction ?? "").not.toContain("Review the bilag metadata");
      expect(rejectedEx!.requiredAction ?? "").not.toContain("re-ingest");
      expect(rejectedEx!.message.toLowerCase()).toContain("afvist");
    } finally {
      cleanupDir(root, inbox);
    }
  });

  test("the no-account-rule exception is Danish, not English", () => {
    const root = freshCompany();
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-302-norule-inbox-"));
    try {
      // A confidently-matchable purchase from a supplier with no account
      // rule — the agent refuses to guess an account and routes
      // AGENT_NO_ACCOUNT_RULE.
      const bilag = join(inbox, "ukendt-leverandoer.txt");
      writeFileSync(bilag, "Faktura\n1250 DKK\n");
      writeFileSync(join(inbox, "ukendt-leverandoer.json"), JSON.stringify({
        source: "email",
        issueDate: "2026-05-12",
        invoiceNo: "UL-302-1",
        deliveryDescription: "Konsulentydelse",
        amountIncVat: 1250,
        currency: "DKK",
        sender: { name: "Ukendt Leverandoer ApS", address: "Vej 9", vatOrCvr: "DK99887766" },
        recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        vatAmount: 250,
        paymentDetails: "Bank transfer",
      }));
      const csv = join(root, "bank-302-norule.csv");
      // Bank text carries the invoice number so the match is confident.
      writeFileSync(csv, [
        "transaction_date,booking_date,text,amount,currency,reference",
        "2026-05-13,2026-05-13,Ukendt Leverandoer ApS faktura UL-302-1,-1250,DKK,UL-302-1",
      ].join("\n"));

      const report = runAgentLoop({
        companyRoot: root,
        asOf: "2026-05-20",
        inboxDir: inbox,
        bankCsvPath: csv,
      });

      const noRuleEx = report.openExceptions.find((x) => x.type === "AGENT_NO_ACCOUNT_RULE");
      expect(noRuleEx).toBeDefined();
      expect(noRuleEx!.message).not.toContain("Bank transaction");
      expect(noRuleEx!.message).not.toContain("confidently matches document");
      expect(noRuleEx!.message).not.toContain("the agent will not guess");
      expect(noRuleEx!.requiredAction ?? "").not.toContain("Add an account rule");
      expect(noRuleEx!.requiredAction ?? "").not.toContain("book the expense manually");
      expect(noRuleEx!.message.toLowerCase()).toContain("kontoregel");
    } finally {
      cleanupDir(root, inbox);
    }
  });

  test("the low-confidence-match exception message source is Danish", () => {
    // AGENT_LOW_CONFIDENCE_MATCH is hard to trigger deterministically through
    // the bank matcher (it clamps amount-only matches below the 0.5 cutoff and
    // corroborated purchase matches reach >= 0.65). Guard the message text
    // itself so an English regression is caught.
    const loopSrc = require("node:fs").readFileSync(
      join(import.meta.dir, "..", "..", "src", "agent", "loop.ts"),
      "utf8",
    ) as string;
    const idx = loopSrc.indexOf("AGENT_LOW_CONFIDENCE_MATCH");
    expect(idx).toBeGreaterThan(-1);
    const block = loopSrc.slice(idx, idx + 700);
    expect(block).not.toContain("Bank transaction ");
    expect(block).not.toContain("matched document");
    expect(block).not.toContain("auto-book threshold");
    expect(block).not.toContain("Review the suggested match");
    expect(block).toContain("Banktransaktion");
  });
});
