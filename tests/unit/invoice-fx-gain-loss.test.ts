// Tests: src/core/invoice-payments.ts (realised exchange gain/loss on settlement)
//
// When a foreign-currency issued invoice is settled at a payment-date rate that
// differs from the invoice-date rate, the receivable (1100) must be relieved at
// the rate it was BOOKED at, and the difference vs the DKK actually received is
// a realised exchange gain (1020) or loss (3320). The receivable must net to
// exactly zero per invoice; the FX difference must reach the P&L.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { importBankCsv } from "../../src/core/bank";
import { issueInvoice } from "../../src/core/issued-invoices";
import { applyInvoicePayment, getInvoiceStatus } from "../../src/core/invoice-payments";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";
import { settleInvoiceFromBank } from "../../src/core/invoice-settlement";
import { issueCreditNote } from "../../src/core/credit-notes";
import { refundInvoiceToBank } from "../../src/core/invoice-refunds";
import { registerInvoiceLateCompensation } from "../../src/core/invoice-compensation";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function netOnAccount(db: any, accountNo: string): number {
  const row = db.query(
    `SELECT COALESCE(SUM(jl.debit_amount), 0) - COALESCE(SUM(jl.credit_amount), 0) AS net
     FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
     WHERE a.account_no = ?`
  ).get(accountNo) as { net: number };
  // SQLite SUMs NUMERIC columns in floating point, so round to øre: the stored
  // per-line amounts are exact, only the aggregation introduces ~1e-13 noise.
  return Math.round(Number(row.net) * 100) / 100;
}

function setup(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function issueEurInvoice(db: any, root: string, invoiceRate: number) {
  // 125 EUR gross (100 net + 25 VAT), booked at `invoiceRate`.
  const grossDkk = Math.round(125 * invoiceRate * 100) / 100;
  const netDkk = Math.round(100 * invoiceRate * 100) / 100;
  const vatDkk = Math.round(grossDkk * 100 - netDkk * 100) / 100;
  const issued = issueInvoice(db, root, {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    dueDate: "2026-06-15",
    invoiceNumber: "2026-0001",
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde GmbH", address: "Berlin", vatOrCvr: "DE811234567" },
    lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
    totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125, fxRateToDkk: invoiceRate, netAmountDkk: netDkk, vatAmountDkk: vatDkk, grossAmountDkk: grossDkk },
    currency: "EUR",
  });
  expect(issued.ok).toBe(true);
  expect(postIssuedInvoiceToLedger(db, { invoiceDocumentId: issued.documentId! }).ok).toBe(true);
  return issued.documentId! as number;
}

function payEur(db: any, root: string, csvName: string, paymentRate: number, ref: string) {
  const amountDkk = Math.round(125 * paymentRate * 100) / 100;
  const csvPath = join(root, csvName);
  writeFileSync(
    csvPath,
    `transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-05-20,2026-05-20,Customer payment,125,EUR,${amountDkk},${paymentRate},${ref}\n`
  );
  expect(importBankCsv(db, root, csvPath).ok).toBe(true);
  return db.query("SELECT id FROM bank_transactions WHERE reference = ?").get(ref) as { id: number };
}

describe("realised exchange gain/loss on invoice settlement", () => {
  test("EUR strengthens between invoice (7.45) and payment (7.46): books a realised exchange GAIN and nets receivable to zero", () => {
    const { root, db } = setup("rentemester-fx-gain-");
    const invoiceId = issueEurInvoice(db, root, 7.45); // receivable debited 931.25 DKK
    const bankTx = payEur(db, root, "bank-gain.csv", 7.46, "INV-GAIN"); // bank receives 932.5 DKK

    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: bankTx.id });
    expect(settled.ok).toBe(true);

    // The whole point: the receivable must net to exactly zero across all entries.
    expect(netOnAccount(db, "1100")).toBe(0);
    // Customer paid 1.25 DKK more than the receivable was carried at → kursgevinst.
    expect(netOnAccount(db, "1020")).toBe(-1.25); // income account: credit balance shows as negative net
    // Bank received the real DKK.
    expect(netOnAccount(db, "2000")).toBe(932.5);

    expect(getInvoiceStatus(db, invoiceId).status).toBe("paid");
    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("EUR weakens between invoice (7.45) and payment (7.44): books a realised exchange LOSS and nets receivable to zero", () => {
    const { root, db } = setup("rentemester-fx-loss-");
    const invoiceId = issueEurInvoice(db, root, 7.45); // receivable debited 931.25 DKK
    const bankTx = payEur(db, root, "bank-loss.csv", 7.44, "INV-LOSS"); // bank receives 930 DKK

    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: bankTx.id });
    expect(settled.ok).toBe(true);

    expect(netOnAccount(db, "1100")).toBe(0);
    // Customer paid 1.25 DKK less than the receivable was carried at → kurstab (expense, debit).
    expect(netOnAccount(db, "3320")).toBe(1.25);
    expect(netOnAccount(db, "2000")).toBe(930);

    expect(getInvoiceStatus(db, invoiceId).status).toBe("paid");
    expect(verifyAuditChain(db).ok).toBe(true);

    db.close();
    cleanupDir(root);
  });

  test("foreign credit note then closing payment: receivable nets to zero and only the true rate drift hits FX (adversarial #6/#7/#8)", () => {
    const { root, db } = setup("rentemester-fx-cn-");
    const invoiceId = issueEurInvoice(db, root, 7.45); // 1100 debited 931.25 DKK
    // 25 EUR foreign credit note → relieves 1100 by 25*7.45 = 186.25 → 1100 = 745.00, open 100 EUR.
    expect(issueCreditNote(db, root, { originalInvoiceDocumentId: invoiceId, issueDate: "2026-05-18", reason: "Delvis kreditering", grossAmount: 25 }).ok).toBe(true);

    const bankTx = payEur(db, root, "bank-cn.csv", 7.46, "INV-CN"); // bank tx is 125 EUR / 932.5 DKK...
    // ...but only 100 EUR remains open; settle exactly the open foreign balance.
    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: bankTx.id, amount: 100 });
    expect(settled.ok).toBe(true);

    // The whole point of the fix: 1100 must net to ZERO, not -186.25.
    expect(netOnAccount(db, "1100")).toBe(0);
    // True realised result: 100 EUR * (7.46 - 7.45) = 1.00 DKK GAIN — NOT a 185 DKK phantom loss.
    expect(netOnAccount(db, "1020")).toBe(-1.0);
    expect(netOnAccount(db, "3320")).toBe(0);

    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("many flat-rate partials (no rate movement) manufacture no phantom FX (adversarial #4)", () => {
    const { root, db } = setup("rentemester-fx-partials-");
    const invoiceId = issueEurInvoice(db, root, 7.45); // 1100 = 931.25
    // Pay the 125 EUR in five 25-EUR partials, every one at the SAME rate 7.45.
    for (let i = 0; i < 5; i++) {
      const csvPath = join(root, `bank-part-${i}.csv`);
      writeFileSync(
        csvPath,
        `transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-05-2${i},2026-05-2${i},Partial,25,EUR,186.25,7.45,PART-${i}\n`
      );
      expect(importBankCsv(db, root, csvPath).ok).toBe(true);
      const tx = db.query("SELECT id FROM bank_transactions WHERE reference = ?").get(`PART-${i}`) as { id: number };
      expect(applyInvoicePayment(db, { invoiceDocumentId: invoiceId, bankTransactionId: tx.id, paymentDate: `2026-05-2${i}`, amount: 25 }).ok).toBe(true);
    }
    // No rate ever moved → no realised gain/loss at all, and 1100 fully relieved.
    expect(netOnAccount(db, "1100")).toBe(0);
    expect(netOnAccount(db, "1020")).toBe(0);
    expect(netOnAccount(db, "3320")).toBe(0);
    expect(getInvoiceStatus(db, invoiceId).status).toBe("paid");
    db.close();
    cleanupDir(root);
  });

  test("combined settlement (principal + claim) of a FOREIGN invoice is rejected, not silently mis-booked (adversarial #1/#5/#9)", () => {
    const { root, db } = setup("rentemester-fx-combined-");
    const invoiceId = issueEurInvoice(db, root, 7.45);
    // Register a late-compensation claim (a flat 310 DKK) so claimOpenBalance > principal,
    // which makes a receipt above the EUR principal take the combined branch.
    const reg = registerInvoiceLateCompensation(db, { invoiceDocumentId: invoiceId, asOfDate: "2026-08-01" });
    expect(reg.ok).toBe(true);

    // A single EUR receipt above the 125 EUR principal → drives the combined branch.
    const csvPath = join(root, "bank-combined.csv");
    writeFileSync(
      csvPath,
      `transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-08-05,2026-08-05,Combined,126,EUR,939.96,7.46,INV-COMBINED\n`
    );
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);
    const tx = db.query("SELECT id FROM bank_transactions WHERE reference = 'INV-COMBINED'").get() as { id: number };

    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: tx.id });
    expect(settled.ok).toBe(false);
    expect(settled.errors.join(" ")).toMatch(/fremmed valuta|foreign-currency/i);
    // Nothing partial was booked.
    expect(db.query("SELECT COUNT(*) AS n FROM invoice_claim_payments").get()).toEqual({ n: 0 });
    db.close();
    cleanupDir(root);
  });

  test("a foreign invoice payment with a caller-provided journalEntryId is rejected (FX could be unhandled) (adversarial #2)", () => {
    const { root, db } = setup("rentemester-fx-jeid-");
    const invoiceId = issueEurInvoice(db, root, 7.45);
    const bookingEntry = db.query("SELECT id FROM journal_entries WHERE document_id = ? LIMIT 1").get(invoiceId) as { id: number };

    const res = applyInvoicePayment(db, {
      invoiceDocumentId: invoiceId,
      journalEntryId: bookingEntry.id,
      paymentDate: "2026-05-20",
      amount: 125,
    });
    expect(res.ok).toBe(false);
    expect(res.errors.join(" ")).toMatch(/fremmed valuta|foreign-currency|journalEntryId/i);
    db.close();
    cleanupDir(root);
  });

  test("awkward-rate multi-partial settlement nets the receivable to EXACTLY zero (adversarial re-review #1/#2)", () => {
    const { root, db } = setup("rentemester-fx-awkward-");
    // 125 EUR @ 7.4567 → grossAmountDkk = round(932.0875) = 932.09 debited to 1100.
    const invoiceId = issueEurInvoice(db, root, 7.4567);
    // Pay in ten 12.5-EUR partials, all at the SAME rate 7.4567 (no rate movement).
    // Per-partial relief is rounded, but the telescoping makes the sum exact.
    for (let i = 0; i < 10; i++) {
      const csvPath = join(root, `bank-awk-${i}.csv`);
      writeFileSync(
        csvPath,
        `transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-05-${10 + i},2026-05-${10 + i},Partial,12.5,EUR,93.21,7.4567,AWK-${i}\n`
      );
      expect(importBankCsv(db, root, csvPath).ok).toBe(true);
      const tx = db.query("SELECT id FROM bank_transactions WHERE reference = ?").get(`AWK-${i}`) as { id: number };
      expect(applyInvoicePayment(db, { invoiceDocumentId: invoiceId, bankTransactionId: tx.id, paymentDate: `2026-05-${10 + i}`, amount: 12.5 }).ok).toBe(true);
    }
    // The must-fix invariant: the receivable control account nets to EXACTLY zero.
    expect(netOnAccount(db, "1100")).toBe(0);
    expect(getInvoiceStatus(db, invoiceId).status).toBe("paid");
    expect(verifyAuditChain(db).ok).toBe(true);
    db.close();
    cleanupDir(root);
  });

  test("refund of a FOREIGN-currency invoice is rejected (refund path is not currency-aware) (adversarial re-review #3)", () => {
    const { root, db } = setup("rentemester-fx-refund-");
    const invoiceId = issueEurInvoice(db, root, 7.45);
    // An outgoing (negative) bank transaction — a refund candidate.
    const csvPath = join(root, "bank-refund.csv");
    writeFileSync(
      csvPath,
      `transaction_date,booking_date,text,amount,currency,amount_dkk,fx_rate_to_dkk,reference\n2026-06-01,2026-06-01,Refund,-125,EUR,-931.25,7.45,REFUND\n`
    );
    expect(importBankCsv(db, root, csvPath).ok).toBe(true);
    const tx = db.query("SELECT id FROM bank_transactions WHERE reference = 'REFUND'").get() as { id: number };

    const refund = refundInvoiceToBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: tx.id });
    expect(refund.ok).toBe(false);
    expect(refund.errors.join(" ")).toMatch(/fremmed valuta|foreign-currency/i);
    db.close();
    cleanupDir(root);
  });

  test("no rate drift (7.46 == 7.46): no FX line, receivable nets to zero", () => {
    const { root, db } = setup("rentemester-fx-flat-");
    const invoiceId = issueEurInvoice(db, root, 7.46);
    const bankTx = payEur(db, root, "bank-flat.csv", 7.46, "INV-FLAT");

    const settled = settleInvoiceFromBank(db, { invoiceDocumentId: invoiceId, bankTransactionId: bankTx.id });
    expect(settled.ok).toBe(true);

    expect(netOnAccount(db, "1100")).toBe(0);
    expect(netOnAccount(db, "1020")).toBe(0);
    expect(netOnAccount(db, "3320")).toBe(0);
    // Entry stays a plain 2-line settlement.
    const lines = db.query(
      `SELECT a.account_no FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(settled.entryId!) as Array<{ account_no: string }>;
    expect(lines.map((l) => l.account_no)).toEqual(["2000", "1100"]);

    db.close();
    cleanupDir(root);
  });
});
