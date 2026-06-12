// Tests: src/core/invoice-interest.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { applyInvoicePayment, getInvoiceStatus } from "../../src/core/invoice-payments";
import { calculateInvoiceLateInterest, postInvoiceLateInterestToLedger, registerInvoiceLateInterest, proposeInterestCorrection, postInterestCorrection } from "../../src/core/invoice-interest";
import { issueCreditNote } from "../../src/core/credit-notes";
import { importBankCsv } from "../../src/core/bank";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { settleInvoiceClaimsFromBank } from "../../src/core/invoice-claim-settlement";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

function failingInterestPostingDb(realDb: any) {
  let failed = false;
  return new Proxy(realDb, {
    get(target, prop, receiver) {
      if (prop === "run") {
        return (sql: string, ...args: any[]) => {
          if (!failed && typeof sql === "string" && sql.includes("INSERT INTO invoice_interest_postings")) {
            failed = true;
            throw new Error("simulated interest posting link failure");
          }
          return target.run(sql, ...args);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as any;
}

describe("invoice late interest", () => {
  test("calculates statutory late interest on overdue open balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    const interest = calculateInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
    });
    expect(interest.ok).toBe(true);
    expect(interest.overdueDays).toBe(5);
    expect(interest.principalOpenBalance).toBe(250);
    expect(interest.annualInterestRatePercent).toBe(10.2);
    expect(interest.accruedInterestAmount).toBe(0.35);
    expect(interest.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-001");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("registers immutable late-interest claims and surfaces them in claim balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-register-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    const registered = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
      note: "First registered interest"
    });
    expect(registered.ok).toBe(true);
    expect(registered.claimId).toBeDefined();
    expect(registered.accruedInterestAmount).toBe(0.35);
    expect(registered.claimOpenBalance).toBe(250.35);
    expect(registered.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-REGISTER-001");

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-20");
    expect(status.ok).toBe(true);
    expect(status.totalInterestClaims).toBe(0.35);
    expect(status.claimOpenBalance).toBe(250.35);
    expect(status.interestClaims).toHaveLength(1);
    expect(status.interestClaims?.[0]?.amountDkk).toBe(0.35);
    expect(status.interestClaims?.[0]?.journalEntryId).toBe(null);

    const duplicate = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
    });
    expect(duplicate.ok).toBe(false);
    expect(duplicate.errors[0]).toContain("already registered");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a second claim at a later date bills ONLY the incremental period, never re-billing days an earlier claim already covered", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-incremental-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);

    // Claim 1 covers the first 5 overdue days (2026-06-15 → 2026-06-20).
    const first = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
    });
    expect(first.ok).toBe(true);
    expect(first.accruedInterestAmount).toBe(0.35);
    expect(first.priorClaimedInterest).toBe(0);

    // Claim 2 at a later date. Morarente accrues continuously on the unpaid
    // principal (renteloven § 3, § 5), so this claim must cover ONLY the new
    // 30 days since the last claim (2026-06-20 → 2026-07-20), not the full
    // 35-day window from the due date — re-billing the first 5 days would be
    // an unlawful double-charge.
    const second = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-07-20",
      referenceRatePercent: 2.2,
    });
    expect(second.ok).toBe(true);
    // 250 kr @ 10.2 % for 30 days = 2.10 kr (NOT 2.45 for the full 35-day window).
    expect(second.accruedInterestAmount).toBe(2.10);
    expect(second.interestFromDate).toBe("2026-06-20");
    expect(second.claimableDays).toBe(30);
    expect(second.priorClaimedInterest).toBe(0.35);
    // Cumulative interest to date equals a single full-window calculation:
    // 0.35 + 2.10 = 2.45 = 250 kr @ 10.2 % for 35 days. No double-count.
    expect(second.totalInterestToDate).toBe(2.45);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-07-20");
    expect(status.interestClaims).toHaveLength(2);
    expect(status.totalInterestClaims).toBe(2.45);
    // The claim balance reflects the correct cumulative interest, not 2.80.
    expect(status.claimOpenBalance).toBe(252.45);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a second claim registered AFTER the first is POSTED also bills only the incremental period (regression: posted double-charge)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-posted-double-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    // Round numbers make the double-charge unmissable: 100.000 kr principal,
    // reference rate 2 % → statutory annual rate 10 %.
    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Konsulentydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1 for the first 30 overdue days, then POST it to the ledger.
    const first = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-03-02",
      referenceRatePercent: 2,
    });
    expect(first.ok).toBe(true);
    expect(first.accruedInterestAmount).toBe(821.92); // 100000 @ 10 % for 30 days
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    // With claim 1 POSTED, the old open-claim guard no longer applied, so a
    // second claim used to recompute the FULL 60-day window (1.643,84) on top
    // of claim 1 — billing the first 30 days twice (total 2.465,76). It must
    // instead bill only the new 30 days.
    const second = registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-04-01",
      referenceRatePercent: 2,
    });
    expect(second.ok).toBe(true);
    expect(second.accruedInterestAmount).toBe(821.92); // the new 30 days only
    expect(second.claimableDays).toBe(30);
    expect(second.priorClaimedInterest).toBe(821.92);
    expect(second.totalInterestToDate).toBe(1643.84);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-04-01");
    // Cumulative interest = full 60-day window, NOT 2.465,76.
    expect(status.totalInterestClaims).toBe(1643.84);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("posts a registered late-interest claim once to receivables and non-VAT claim income", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-post-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(applyInvoicePayment(db, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);
    expect(registerInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
      note: "First registered interest"
    }).ok).toBe(true);

    const posted = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(posted.ok).toBe(true);
    expect(posted.accruedInterestAmount).toBe(0.35);
    expect(posted.appliedRules).toContain("DK-INVOICE-LATE-INTEREST-BOOKKEEPING-001");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1100", debit_amount: 0.35, credit_amount: 0, vat_code: null },
      { account_no: "1010", debit_amount: 0, credit_amount: 0.35, vat_code: null },
    ]);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-06-20");
    expect(status.ok).toBe(true);
    expect(status.interestClaims?.[0]?.journalEntryId).toBe(posted.entryId);

    const second = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("already posted");

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("rolls back the journal entry if interest posting link creation fails", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-atomic-"));
    const realDb = openDb(ensureCompanyDirs(root).db);
    migrate(realDb);
    seedAccounts(realDb);
    const db = failingInterestPostingDb(realDb);

    const issued = issueInvoice(realDb, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-06-15",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(applyInvoicePayment(realDb, {
      invoiceDocumentId: issued.documentId!,
      paymentDate: "2026-05-20",
      amount: 1000,
      note: "Partial payment"
    }).ok).toBe(true);
    expect(registerInvoiceLateInterest(realDb, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-20",
      referenceRatePercent: 2.2,
    }).ok).toBe(true);

    const failed = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(failed.ok).toBe(false);
    expect(failed.errors[0]).toContain("simulated interest posting link failure");
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 1 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_interest_postings").get()).toEqual({ n: 0 });

    const retry = postInvoiceLateInterestToLedger(realDb, { invoiceDocumentId: issued.documentId! });
    expect(retry.ok).toBe(true);
    expect(realDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 2 });
    expect(realDb.query("SELECT COUNT(*) AS n FROM invoice_interest_postings").get()).toEqual({ n: 1 });

    realDb.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("returns zero interest for non-overdue or fully settled invoices", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-zero-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const interest = calculateInvoiceLateInterest(db, {
      invoiceDocumentId: issued.documentId!,
      asOfDate: "2026-06-10",
      referenceRatePercent: 2.2,
    });
    expect(interest.ok).toBe(true);
    expect(interest.overdueDays).toBe(0);
    expect(interest.accruedInterestAmount).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("staged claims sum to a single round-once calculation — no per-segment øre over-charge", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-drift-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    // 1000 kr open balance, reference rate 10.5 → statutory annual rate 18.5 %.
    // Two consecutive single-day claims each round to 0.51 in isolation (→ 1.02),
    // but a single 2-day calculation is 1.01. The round-once cumulative must bill
    // 1.01 in total, never 1.02 — otherwise staged claims re-bill a fractional øre.
    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 800, lineTotalExVat: 800 }],
      totals: { netAmount: 800, vatRate: 0.25, vatAmount: 200, grossAmount: 1000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-02-01", referenceRatePercent: 10.5 });
    expect(c1.ok).toBe(true);
    expect(c1.accruedInterestAmount).toBe(0.51);

    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-02-02", referenceRatePercent: 10.5 });
    expect(c2.ok).toBe(true);
    // Drift-corrected increment: 1.01 cumulative − 0.51 already billed = 0.50,
    // NOT a second 0.51 (which would over-charge by 1 øre).
    expect(c2.accruedInterestAmount).toBe(0.50);
    expect(c2.totalInterestToDate).toBe(1.01);

    const status = getInvoiceStatus(db, issued.documentId!, "2026-02-02");
    expect(status.totalInterestClaims).toBe(1.01);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("calculating at an as-of date BEFORE the latest claim reports interest only through that date (no over-report, no future from-date)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-backward-"));
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
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // A claim already registered well into the future (35 days overdue → 12,23 kr).
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-07-20", referenceRatePercent: 2.2 }).ok).toBe(true);

    // Querying an EARLIER as-of date must report interest through 2026-06-20
    // (~5 days ≈ 1,75 kr), NOT the 12,23 kr accrued through the later claim, and
    // the reported from-date must not be after the as-of date.
    const back = calculateInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-06-20", referenceRatePercent: 2.2 });
    expect(back.ok).toBe(true);
    expect(back.accruedInterestAmount).toBe(0);
    expect(back.claimableDays).toBe(0);
    expect(back.interestFromDate).toBe("2026-06-20"); // clamped to the as-of date, never later
    expect(back.totalInterestToDate).toBe(1.75);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("date-aware accrual: a partial payment splits the later window — days before the payment accrue on the full principal, days after on the reduced one", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-partial-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    // 100.000 kr principal, statutory annual rate 10 %.
    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1: 30 days on the full 100.000 kr.
    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-02", referenceRatePercent: 2 });
    expect(c1.ok).toBe(true);
    expect(c1.accruedInterestAmount).toBe(821.92);

    // A 40.000 kr partial payment effective 2026-03-10 lowers the balance to 60.000.
    expect(applyInvoicePayment(db, { invoiceDocumentId: issued.documentId!, paymentDate: "2026-03-10", amount: 40000, note: "Afdrag" }).ok).toBe(true);

    // Claim 2 covers 2026-03-02 → 2026-04-01 (30 days), but date-aware: the first
    // 8 days (until the payment on 2026-03-10) accrue on 100.000 kr, the next 22
    // on 60.000 kr — NOT a flat 30 days on the reduced balance (which would be the
    // wrong 493,15 kr), and NOT 30 days on the original 100.000.
    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-04-01", referenceRatePercent: 2 });
    expect(c2.ok).toBe(true);
    expect(c2.principalOpenBalance).toBe(60000); // balance as of the as-of date
    // 100.000 @ 10 % × 8 d + 60.000 @ 10 % × 22 d = 580,82 kr.
    expect(c2.accruedInterestAmount).toBe(580.82);
    expect(c2.totalInterestToDate).toBe(1402.74); // 821,92 + 580,82

    const status = getInvoiceStatus(db, issued.documentId!, "2026-04-01");
    expect(status.totalInterestClaims).toBe(1402.74);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("date-aware accrual also reflects a CREDIT NOTE (not just payments) by its effective date", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-creditnote-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-02", referenceRatePercent: 2 });
    expect(c1.ok).toBe(true);
    expect(c1.accruedInterestAmount).toBe(821.92);

    // A 40.000 kr partial credit note effective 2026-03-10 lowers the balance —
    // same effect as a payment, via a different event source.
    expect(issueCreditNote(db, root, { originalInvoiceDocumentId: issued.documentId!, issueDate: "2026-03-10", reason: "Delvis kreditering", grossAmount: 40000 }).ok).toBe(true);

    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-04-01", referenceRatePercent: 2 });
    expect(c2.ok).toBe(true);
    // Same date-aware split as the payment case: 8 d @ 100.000 + 22 d @ 60.000.
    expect(c2.accruedInterestAmount).toBe(580.82);
    expect(c2.totalInterestToDate).toBe(1402.74);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a BACK-DATED payment (effective before an existing claim) does not stack a new over-charge — it clamps to 0 and surfaces overClaimedInterest", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-backdated-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1 billed 59 days on the full 100.000 (no payment known yet) = 1.616,44.
    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-31", referenceRatePercent: 2 });
    expect(c1.ok).toBe(true);
    expect(c1.accruedInterestAmount).toBe(1616.44);

    // A 50.000 payment is now recorded with a BACK-DATED effective date inside
    // claim 1's already-billed window.
    expect(applyInvoicePayment(db, { invoiceDocumentId: issued.documentId!, paymentDate: "2026-02-15", amount: 50000, note: "Bagud-dateret afdrag" }).ok).toBe(true);

    // The now-lawful cumulative through 2026-05-01 (date-aware: 100.000 for 15 d,
    // then 50.000) is 1.438,36 — LESS than the 1.616,44 already billed. A new claim
    // must NOT stack more interest on top: it clamps to 0 and reports the 178,08
    // over-claim so a correcting credit can be considered.
    const calc = calculateInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-05-01", referenceRatePercent: 2 });
    expect(calc.ok).toBe(true);
    expect(calc.totalInterestToDate).toBe(1438.36);
    expect(calc.accruedInterestAmount).toBe(0);
    expect(calc.overClaimedInterest).toBe(178.08);

    // Registration is refused (nothing new is lawfully owed) — no over-charge stacks.
    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-05-01", referenceRatePercent: 2 });
    expect(c2.ok).toBe(false);
    expect(c2.errors[0]).toContain("positive");

    const status = getInvoiceStatus(db, issued.documentId!, "2026-05-01");
    expect(status.totalInterestClaims).toBe(1616.44); // only claim 1; nothing stacked

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a back-dated over-claim can be proposed and booked as a correcting reversal, netting the interest-claim balance", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-correction-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1 billed + POSTED on the full 100.000 (1.616,44).
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-31", referenceRatePercent: 2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    // A 50.000 payment is recorded BACK-DATED to 2026-02-15 (inside claim 1's window).
    expect(applyInvoicePayment(db, { invoiceDocumentId: issued.documentId!, paymentDate: "2026-02-15", amount: 50000, note: "Bagud-dateret afdrag" }).ok).toBe(true);

    // Proposal: posted 1.616,44 vs lawful 1.013,70 → over-claimed 602,74.
    const proposal = proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(proposal.ok).toBe(true);
    expect(proposal.hasProposal).toBe(true);
    expect(proposal.postedInterest).toBe(1616.44);
    expect(proposal.lawfulInterest).toBe(1013.70);
    expect(proposal.overClaimedAmount).toBe(602.74);
    expect(proposal.throughDate).toBe("2026-03-31");

    // Book the correction: debit interest income (1010), credit receivable (1100).
    const correction = postInterestCorrection(db, { invoiceDocumentId: issued.documentId!, reason: "Bagud-dateret betaling" });
    expect(correction.ok).toBe(true);
    expect(correction.correctedAmount).toBe(602.74);
    expect(correction.correctionId).toBeDefined();

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(correction.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "1010", debit_amount: 602.74, credit_amount: 0, vat_code: null },
      { account_no: "1100", debit_amount: 0, credit_amount: 602.74, vat_code: null },
    ]);

    // Status nets the correction off the interest-claim balance.
    const status = getInvoiceStatus(db, issued.documentId!, "2026-03-31");
    expect(status.totalInterestCorrections).toBe(602.74);
    expect(status.totalInterestClaims).toBe(1013.70); // 1616,44 − 602,74

    // Idempotent: nothing left to correct.
    const reProposal = proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(reProposal.hasProposal).toBe(false);
    expect(reProposal.overClaimedAmount).toBe(0);
    const second = postInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(second.ok).toBe(false);
    expect(second.errors[0]).toContain("no over-claimed");

    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("a later interest claim after a correction re-bills the corrected period — the reversed amount is not permanently lost", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-correction-thenclaim-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1 posted on the full 100.000 (1.616,44), back-dated 50.000 payment,
    // then the over-claim corrected (602,74 reversed → net booked 1.013,70).
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-31", referenceRatePercent: 2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(applyInvoicePayment(db, { invoiceDocumentId: issued.documentId!, paymentDate: "2026-02-15", amount: 50000, note: "Bagud-dateret afdrag" }).ok).toBe(true);
    expect(postInterestCorrection(db, { invoiceDocumentId: issued.documentId! }).correctedAmount).toBe(602.74);

    // A later claim on the remaining 50.000 through 2026-05-30 must bill against
    // the NET booked interest (1.013,70), not the raw 1.616,44: lawful-through-05-30
    // 1.835,62 − 1.013,70 = 821,92 — NOT 219,18 (which would lose the reversed amount).
    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-05-30", referenceRatePercent: 2 });
    expect(c2.ok).toBe(true);
    expect(c2.accruedInterestAmount).toBe(821.92);

    // Net booked interest = 1.616,44 − 602,74 + 821,92 = 1.835,62 = the lawful total.
    const status = getInvoiceStatus(db, issued.documentId!, "2026-05-30");
    expect(status.totalInterestClaims).toBe(1835.62);
    // And nothing is over-claimed any more.
    expect(proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! }).hasProposal).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("proposeInterestCorrection measures each POSTED claim against its OWN window, so a later posted claim with an earlier UNPOSTED one is not masked", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-noncontiguous-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    // Claim 1 registered but LEFT UNPOSTED; claim 2 (its incremental window) POSTED.
    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-02", referenceRatePercent: 2 });
    expect(c1.ok).toBe(true);
    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-04-01", referenceRatePercent: 2 });
    expect(c2.ok).toBe(true);
    expect(c2.accruedInterestAmount).toBe(821.92);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId!, claimId: c2.claimId }).ok).toBe(true);

    // Back-dated credit note halves the balance from 2026-02-15.
    expect(issueCreditNote(db, root, { originalInvoiceDocumentId: issued.documentId!, issueDate: "2026-02-15", reason: "Delvis kreditering", grossAmount: 50000 }).ok).toBe(true);

    // The over-claim must be measured on claim 2's OWN window (2026-03-02→2026-04-01
    // on the reduced 50.000): lawful 410,96 vs posted 821,92 → 410,96. The old
    // contiguous-chain logic folded claim 1's unposted window into lawful and
    // wrongly reported 0.
    const proposal = proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(proposal.ok).toBe(true);
    expect(proposal.postedInterest).toBe(821.92);
    expect(proposal.lawfulInterest).toBe(410.96);
    expect(proposal.overClaimedAmount).toBe(410.96);
    expect(proposal.requiresRefund).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("postInterestCorrection REFUSES when the over-claimed interest was already settled in cash (a refund, not a receivable credit, is required)", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-settled-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-31", referenceRatePercent: 2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    // Customer pays the principal in full, THEN pays the interest claim in cash.
    const principalCsv = join(root, "principal.csv");
    writeFileSync(principalCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-04-01,2026-04-01,Principal,100000,DKK,INV-PRIN\n");
    expect(importBankCsv(db, root, principalCsv).ok).toBe(true);
    const prinTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-PRIN'").get() as { id: number };
    expect(settleInvoiceFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: prinTx.id }).ok).toBe(true);

    const claimCsv = join(root, "claim.csv");
    writeFileSync(claimCsv, "transaction_date,booking_date,text,amount,currency,reference\n2026-04-02,2026-04-02,Interest,1616.44,DKK,INV-INT\n");
    expect(importBankCsv(db, root, claimCsv).ok).toBe(true);
    const intTx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-INT'").get() as { id: number };
    expect(settleInvoiceClaimsFromBank(db, { invoiceDocumentId: issued.documentId!, bankTransactionId: intTx.id }).ok).toBe(true);

    // NOW a back-dated credit note surfaces — the booked interest was over-charged,
    // but the customer already PAID it in cash.
    expect(issueCreditNote(db, root, { originalInvoiceDocumentId: issued.documentId!, issueDate: "2026-02-15", reason: "Bagud-dateret kreditnota", grossAmount: 50000 }).ok).toBe(true);

    const proposal = proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(proposal.ok).toBe(true);
    expect(proposal.overClaimedAmount).toBe(602.74);
    expect(proposal.requiresRefund).toBe(true); // over-claim exceeds the (now negative) outstanding balance

    // The correction must NOT book a receivable credit — that would reverse
    // collected income and drive the receivable negative. Refuse, point to a refund.
    const correction = postInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(correction.ok).toBe(false);
    expect(correction.errors[0]).toContain("refund");

    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("proposeInterestCorrection finds nothing to correct on a clean forward-ordered invoice", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-correction-clean-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);
    expect(registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-31", referenceRatePercent: 2 }).ok).toBe(true);
    expect(postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);

    const proposal = proposeInterestCorrection(db, { invoiceDocumentId: issued.documentId! });
    expect(proposal.ok).toBe(true);
    expect(proposal.hasProposal).toBe(false);
    expect(proposal.overClaimedAmount).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("the default post path books the oldest UNPOSTED claim, so a second claim is postable after the first is posted", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-interest-post-default-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const issued = issueInvoice(db, root, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-01-01",
      dueDate: "2026-01-31",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [{ description: "Ydelse", quantity: 1, unitPriceExVat: 80000, lineTotalExVat: 80000 }],
      totals: { netAmount: 80000, vatRate: 0.25, vatAmount: 20000, grossAmount: 100000 },
      currency: "DKK"
    });
    expect(issued.ok).toBe(true);

    const c1 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-03-02", referenceRatePercent: 2 });
    expect(c1.ok).toBe(true);
    const p1 = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(p1.ok).toBe(true);
    expect(p1.claimId).toBe(c1.claimId);

    const c2 = registerInvoiceLateInterest(db, { invoiceDocumentId: issued.documentId!, asOfDate: "2026-04-01", referenceRatePercent: 2 });
    expect(c2.ok).toBe(true);

    // Default path (no claimId) must now post claim 2, not re-select the posted claim 1.
    const p2 = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(p2.ok).toBe(true);
    expect(p2.claimId).toBe(c2.claimId);

    // Both posted; a third default call reports nothing left to post.
    const p3 = postInvoiceLateInterestToLedger(db, { invoiceDocumentId: issued.documentId! });
    expect(p3.ok).toBe(false);
    expect(p3.errors[0]).toContain("already posted");

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
