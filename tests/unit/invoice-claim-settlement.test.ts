// Tests: src/core/invoice-claim-settlement.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { issueInvoice } from "../../src/core/issued-invoices";
import { getInvoiceStatus } from "../../src/core/invoice-payments";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { settleInvoiceClaimsFromBank } from "../../src/core/invoice-claim-settlement";
import { registerInvoiceReminder, postInvoiceReminderToLedger } from "../../src/core/invoice-reminders";
import { registerInvoiceLateCompensation, postInvoiceLateCompensationToLedger } from "../../src/core/invoice-compensation";
import { registerInvoiceLateInterest, postInvoiceLateInterestToLedger } from "../../src/core/invoice-interest";
import { postJournalEntry, reverseJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { confirmAccountRole } from "../../src/core/account-roles";

describe("invoice claim settlement", () => {
  test("rejects a standalone claim receipt when the registered claim is not ledger-posted", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-unposted-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 80, lineTotalExVat: 80 }],
      totals: { netAmount: 80, vatRate: 0.25, vatAmount: 20, grossAmount: 100 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);

    const principalCsv = join(root, "principal-unposted.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-27,2026-06-27,Principal,100,DKK,UNPOSTED-PRINCIPAL\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'UNPOSTED-PRINCIPAL'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    const claimCsv = join(root, "claim-unposted.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim,100,DKK,UNPOSTED-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'UNPOSTED-CLAIM'").get() as { id: number };
    const before = {
      journals: (db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n,
      claims: (db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get() as { n: number }).n,
    };
    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(false);
    expect(settled.errors.join(" ")).toContain("not ledger-posted");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: before.journals });
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get()).toEqual({ n: before.claims });

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("clears the claim account that was actually debited and blocks standalone evidence reversal", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-role-change-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 80, lineTotalExVat: 80 }],
      totals: { netAmount: 80, vatRate: 0.25, vatAmount: 20, grossAmount: 100 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);
    const claimPosting = postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(claimPosting.ok).toBe(true);
    const blockedReversal = reverseJournalEntry(db, {
      entryId: claimPosting.entryId!,
      transactionDate: "2026-06-27",
      reason: "Must be an atomic claim correction",
    });
    expect(blockedReversal.ok).toBe(false);
    expect(blockedReversal.errors.join(" ")).toContain("protected invoice evidence");

    const principalCsv = join(root, "principal-role-change.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-27,2026-06-27,Principal,100,DKK,ROLE-PRINCIPAL\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'ROLE-PRINCIPAL'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    db.run("INSERT INTO accounts (account_no, name, type, normal_balance) VALUES ('1110', 'New debtors', 'asset', 'debit')");
    expect(confirmAccountRole(db, "debtors", "1110", "user:reviewer").ok).toBe(true);
    const claimCsv = join(root, "claim-role-change.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim,100,DKK,ROLE-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'ROLE-CLAIM'").get() as { id: number };
    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(true);
    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
         FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ? ORDER BY jl.id`,
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 100, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 100 },
    ]);
    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("settles booked claim receivables from an imported bank receipt after principal is cleared", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);
    expect(postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceLateCompensation(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-06-20" }).ok).toBe(true);
    expect(postInvoiceLateCompensationToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-06-20", referenceRatePercent: 2.2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const principalCsv = join(root, "bank-principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0990\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    const claimCsv = join(root, "bank-claim.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim payment,411.75,DKK,INV-0990-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990-CLAIM'").get() as { id: number };

    const mismatched = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
      amount: 400,
    });
    expect(mismatched.ok).toBe(false);
    expect(mismatched.errors.join(" ")).toContain("must equal bank transaction");
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get()).toEqual({ n: 0 });

    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(true);
    expect(settled.appliedRules).toContain("DK-INVOICE-CLAIM-SETTLEMENT-001");
    expect(settled.remainingClaimOpenBalance).toBe(0);

    const status = getInvoiceStatus(db, issued.documentId!);
    expect(status.ok).toBe(true);
    expect(status.openBalance).toBe(0);
    expect(status.claimOpenBalance).toBe(0);
    expect(status.totalClaimPayments).toBe(411.75);
    expect(getInvoiceStatus(db, issued.documentId!, "2026-06-27").totalClaimPayments).toBe(0);
    expect(getInvoiceStatus(db, issued.documentId!, "2026-06-27").claimOpenBalance).toBe(411.75);
    expect(getInvoiceStatus(db, issued.documentId!, "2026-06-28").totalClaimPayments).toBe(411.75);
    expect(status.claimPayments).toHaveLength(1);
    expect(status.claimPayments?.[0]?.amount).toBe(411.75);
    expect(status.claimPayments?.[0]?.journalEntryId).toBe(settled.entryId);
    expect(db.query("SELECT journal_entry_id FROM invoice_claim_payments WHERE id = ?").get(settled.claimPaymentId!)).toEqual({ journal_entry_id: settled.entryId });

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "2000", debit_amount: 411.75, credit_amount: 0 },
      { account_no: "1100", debit_amount: 0, credit_amount: 411.75 },
    ]);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("requires explicit bank transaction selection for claim settlement", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-explicit-bank-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);
    expect(postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const principalCsv = join(root, "bank-principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-05-20,2026-05-20,Customer payment,1250,DKK,INV-0990B\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principalTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0990B'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: principalTx.id }).ok).toBe(true);

    const strayClaimCsv = join(root, "bank-claim.csv");
    writeFileSync(strayClaimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Unrelated claim payment,100,DKK,OTHER-CLAIM\n");
    expect(importBankCsv(db, root, strayClaimCsv).ok).toBe(true);

    const settled = settleInvoiceClaimsFromBank(db, { invoiceDocumentId: issued.documentId! });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toBe("bankTransactionId or bankTransactionReference is required");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("blocks claim settlement before principal is cleared", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-blocked-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, { invoiceDocumentId: issued.documentId!, reminderDate: "2026-06-26" }).ok).toBe(true);
    expect(postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const claimCsv = join(root, "bank-claim.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-28,2026-06-28,Claim payment,100,DKK,INV-0991-CLAIM\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const claimTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-0991-CLAIM'").get() as { id: number };

    const settled = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: claimTx.id,
    });
    expect(settled.ok).toBe(false);
    expect(settled.errors[0]).toContain("settle principal before claim receipts");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("blocks standalone, combined, override, and direct claim settlement before the claim exists", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-claim-settle-chronology-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 80, lineTotalExVat: 80 }],
      totals: { netAmount: 80, vatRate: 0.25, vatAmount: 20, grossAmount: 100 },
      currency: "DKK",
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceReminder(db, {
      invoiceDocumentId: issued.documentId!,
      reminderDate: "2026-06-26",
    }).ok).toBe(true);
    expect(postInvoiceReminderToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const earlyCombinedCsv = join(root, "early-combined.csv");
    writeFileSync(earlyCombinedCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-20,2026-06-20,Early combined,200,DKK,EARLY-COMBINED\n");
    expect(importBankCsv(db, root, earlyCombinedCsv).ok).toBe(true);
    const earlyCombined = db.query("SELECT id FROM bank_transactions WHERE reference = 'EARLY-COMBINED'").get() as { id: number };
    const combined = settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: earlyCombined.id,
    });
    expect(combined.ok).toBe(false);
    expect(combined.errors.join(" ")).toContain("not effective by 2026-06-20");

    const principalCsv = join(root, "chronology-principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-27,2026-06-27,Principal,100,DKK,CHRONOLOGY-PRINCIPAL\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const principal = db.query("SELECT id FROM bank_transactions WHERE reference = 'CHRONOLOGY-PRINCIPAL'").get() as { id: number };
    expect(settleInvoiceFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: principal.id,
    }).ok).toBe(true);

    const earlyClaimCsv = join(root, "early-claim.csv");
    writeFileSync(earlyClaimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-06-20,2026-06-20,Early claim,100,DKK,EARLY-CLAIM\n");
    expect(importBankCsv(db, root, earlyClaimCsv).ok).toBe(true);
    const earlyClaim = db.query("SELECT id FROM bank_transactions WHERE reference = 'EARLY-CLAIM'").get() as { id: number };

    const standalone = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: earlyClaim.id,
    });
    expect(standalone.ok).toBe(false);
    expect(standalone.errors.join(" ")).toContain("not effective by 2026-06-20");

    const overridden = settleInvoiceClaimsFromBank(db, {
      invoiceDocumentId: issued.documentId!,
      bankTransactionId: earlyClaim.id,
      paymentDate: "2026-06-28",
    });
    expect(overridden.ok).toBe(false);
    expect(overridden.errors.join(" ")).toContain("not effective by 2026-06-20");

    const journalCount = (db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n;
    expect(() => db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: "2026-06-28",
        text: "Direct backdated-bank claim settlement",
        documentId: issued.documentId!,
        sourceBankTransactionId: earlyClaim.id,
        lines: [
          { accountNo: "2000", debitAmount: 100 },
          { accountNo: "1100", creditAmount: 100 },
        ],
      });
      expect(journal.ok).toBe(true);
      db.run(
        `INSERT INTO invoice_claim_payments
           (invoice_document_id, bank_transaction_id, journal_entry_id,
            payment_date, amount, currency)
         VALUES (?, ?, ?, '2026-06-28', 100, 'DKK')`,
        issued.documentId!,
        earlyClaim.id,
        journal.entryId!,
      );
    })()).toThrow("invoice claim payment cannot predate its active claim evidence");
    expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: journalCount });
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get()).toEqual({ n: 0 });
    expect(verifyAuditChain(db).ok).toBe(true);

    db.exec("DROP TRIGGER invoice_claim_payments_require_journal");
    const legacyJournal = postJournalEntry(db, {
      transactionDate: "2026-06-28",
      text: "Legacy pre-claim bank receipt",
      documentId: issued.documentId!,
      sourceBankTransactionId: earlyClaim.id,
      lines: [
        { accountNo: "2000", debitAmount: 100 },
        { accountNo: "1100", creditAmount: 100 },
      ],
    });
    expect(legacyJournal.ok).toBe(true);
    db.run(
      `INSERT INTO invoice_claim_payments
         (invoice_document_id, bank_transaction_id, journal_entry_id,
          payment_date, amount, currency)
       VALUES (?, ?, ?, '2026-06-28', 100, 'DKK')`,
      issued.documentId!,
      earlyClaim.id,
      legacyJournal.entryId!,
    );
    const legacyStatus = getInvoiceStatus(db, issued.documentId!);
    expect(legacyStatus.ok).toBe(false);
    expect(legacyStatus.errors.join(" ")).toContain("not effective by 2026-06-20");
    const legacyAudit = verifyAuditChain(db);
    expect(legacyAudit.ok).toBe(false);
    expect(legacyAudit.errors.join(" ")).toContain("not effective by 2026-06-20");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
