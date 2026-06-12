// Tests: src/server/data.ts — portfolio aggregation across workspace companies.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { buildPortfolioOverview } from "../../src/server/data";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { recordException } from "../../src/core/exceptions";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function config(workspaceRoot: string): ServerConfig {
  return { host: "127.0.0.1", port: 0, authRequired: false, authToken: null, workspaceRoot };
}

/**
 * Posts a 25%-VAT sale (income) and purchase (expense) pair into a company's
 * ledger — the same shape `postPnlEntry` uses in server-api.test.ts. Income
 * credits account 1000 + 1200 (sales VAT); expense debits 3000 + 4000
 * (purchase VAT); the cash side runs over bank account 2000.
 */
function seedPnl(
  ws: string,
  slug: string,
  date: string,
  income: number,
  expense: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-pf-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    writeFileSync(sourceFile, `Bilag ${slug} ${date}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: date,
      invoiceNo: `PF-${slug}-${date}`,
      deliveryDescription: "Portefølje testbilag",
      amountIncVat: 1,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: slug, address: "Vej 2", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bankoverførsel",
    });
    if (!doc.ok) throw new Error("doc ingest failed: " + (doc.errors ?? []).join("; "));
    if (income > 0) {
      const sale = postJournalEntry(db, {
        transactionDate: date,
        text: "Portefølje salg",
        documentId: doc.documentId,
        lines: [
          { accountNo: "2000", debitAmount: income * 1.25 },
          { accountNo: "1000", creditAmount: income, vatCode: "DK_SALE_25" },
          { accountNo: "1200", creditAmount: income * 0.25 },
        ],
      });
      if (!sale.ok) throw new Error("sale post failed: " + sale.errors.join("; "));
    }
    if (expense > 0) {
      const purchase = postJournalEntry(db, {
        transactionDate: date,
        text: "Portefølje køb",
        documentId: doc.documentId,
        lines: [
          { accountNo: "3000", debitAmount: expense, vatCode: "DK_PURCHASE_25" },
          { accountNo: "4000", debitAmount: expense * 0.25 },
          { accountNo: "2000", creditAmount: expense * 1.25 },
        ],
      });
      if (!purchase.ok) throw new Error("purchase post failed: " + purchase.errors.join("; "));
    }
  } finally {
    db.close();
  }
}

/** Imports a bank-statement row carrying a running `balance_after`. */
function seedBankRow(
  ws: string,
  slug: string,
  date: string,
  amount: number,
  balanceAfter: number,
) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    db.query(
      `INSERT INTO bank_transactions
         (transaction_date, text, amount, currency, transaction_hash, status,
          balance_after)
       VALUES (?, ?, ?, 'DKK', ?, 'imported', ?)`,
    ).run(date, `txn ${date}`, amount, `hash-${slug}-${date}`, balanceAfter);
  } finally {
    db.close();
  }
}

/**
 * Records an open exception of the given type. The message is made unique per
 * call so `recordException`'s same-message dedupe does not collapse rows.
 */
let exceptionSeq = 0;
function seedException(ws: string, slug: string, type: string) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    exceptionSeq += 1;
    recordException(db, {
      type,
      severity: "medium",
      message: `${type} task #${exceptionSeq}`,
    });
  } finally {
    db.close();
  }
}

describe("portfolio aggregation", () => {
  test("an empty workspace yields zero companies and zero totals", () => {
    const ws = tmpRoot("pf-empty");
    try {
      initWorkspace(ws);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.companyCount).toBe(0);
      expect(overview.companies).toEqual([]);
      expect(overview.totals.openInvoiceCount).toBe(0);
      expect(overview.totals.openInvoiceTotal).toBe(0);
    } finally {
      cleanupDir(ws);
    }
  });

  test("produces one summary per company", () => {
    const ws = tmpRoot("pf-summaries");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      createCompany(ws, { name: "Beta IVS" });
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.companyCount).toBe(2);
      const slugs = overview.companies.map((c) => c.slug).sort();
      expect(slugs).toEqual(["acme-aps", "beta-ivs"]);
      for (const c of overview.companies) {
        expect(c.openInvoiceCount).toBe(0);
        expect(c.ledgerMissing).toBe(false);
        expect(c).toHaveProperty("netVatPayable");
        expect(c).toHaveProperty("auditChainOk");
      }
    } finally {
      cleanupDir(ws);
    }
  });

  test("totals are the sum of per-company figures", () => {
    const ws = tmpRoot("pf-totals");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      createCompany(ws, { name: "Beta IVS" });
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const sum = overview.companies.reduce((a, c) => a + c.openInvoiceCount, 0);
      expect(overview.totals.openInvoiceCount).toBe(sum);
      const vatSum = overview.companies.reduce((a, c) => a + c.netVatPayable, 0);
      expect(overview.totals.netVatPayable).toBeCloseTo(vatSum, 6);
    } finally {
      cleanupDir(ws);
    }
  });

  test("a company carries its real current-year resultat, omsætning and VAT", () => {
    const ws = tmpRoot("pf-realfigures");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      // Income 1000, expense 400 → resultat 600; 25%-VAT → payable 150.
      seedPnl(ws, "acme-aps", "2026-03-15", 1000, 400);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const c = overview.companies[0]!;
      expect(c.fiscalYear).toBe("2026");
      expect(c.omsaetning).toBe(1000);
      expect(c.resultat).toBe(600);
      expect(c.vat?.payable).toBe(150);
      // 2026-03-15 falls in Q1 → statutory deadline 1 June 2026.
      expect(c.vat?.deadline).toBe("2026-06-01");
      expect(c.netVatPayable).toBe(150);
    } finally {
      cleanupDir(ws);
    }
  });

  test("VAT uses the booked position, not the old quarter report (0-bug fix)", () => {
    const ws = tmpRoot("pf-vatfix");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      seedPnl(ws, "acme-aps", "2026-02-01", 1000, 0);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      // Output VAT booked on account 1200 (250) with no input → payable 250.
      expect(overview.companies[0]!.vat?.payable).toBe(250);
    } finally {
      cleanupDir(ws);
    }
  });

  test("actual bank balance comes from the imported statement", () => {
    const ws = tmpRoot("pf-bank");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      seedPnl(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankRow(ws, "acme-aps", "2026-04-01", 700, 700);
      seedBankRow(ws, "acme-aps", "2026-04-10", -200, 500);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.companies[0]!.actualBankBalance).toBe(500);
    } finally {
      cleanupDir(ws);
    }
  });

  test("open exceptions are grouped into Danish task lines", () => {
    const ws = tmpRoot("pf-tasks");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedException(ws, "acme-aps", "MAIL_INTAKE_NO_ATTACHMENT");
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const c = overview.companies[0]!;
      expect(c.openTaskCount).toBe(3);
      expect(c.openExceptionCount).toBe(3);
      const bank = c.taskGroups.find((g) => g.type === "UNMATCHED_BANK_TRANSACTION");
      expect(bank?.count).toBe(2);
      expect(bank?.label).toBe("2 banktransaktioner mangler afstemning");
    } finally {
      cleanupDir(ws);
    }
  });

  test("the roll-up sums resultat, liquidity, VAT and tasks across companies", () => {
    const ws = tmpRoot("pf-rollup");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      createCompany(ws, { name: "Beta ApS" });
      seedPnl(ws, "acme-aps", "2026-03-15", 1000, 400); // resultat 600, VAT 150
      seedBankRow(ws, "acme-aps", "2026-04-01", 500, 500);
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedPnl(ws, "beta-aps", "2026-03-15", 2000, 500); // resultat 1500, VAT 375
      seedBankRow(ws, "beta-aps", "2026-04-01", 800, 800);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.rollup.resultat).toBe(2100);
      expect(overview.rollup.liquidity).toBe(1300);
      expect(overview.rollup.vatPayable).toBe(525);
      expect(overview.rollup.openTaskCount).toBe(1);
    } finally {
      cleanupDir(ws);
    }
  });

  test("a company with no ledger on disk degrades gracefully", () => {
    const ws = tmpRoot("pf-noledger");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      // Remove the ledger file to simulate a registered-but-missing company.
      rmSync(companyPaths(companyRootForSlug(ws, "acme-aps")).db, { force: true });
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const c = overview.companies[0]!;
      expect(c.ledgerMissing).toBe(true);
      expect(c.resultat).toBe(0);
      expect(c.vat).toBeNull();
      expect(c.actualBankBalance).toBeNull();
    } finally {
      cleanupDir(ws);
    }
  });

  test("GET /api/portfolio surfaces the aggregation", async () => {
    const ws = tmpRoot("pf-endpoint");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS" });
      const res = await handleRequest(
        new Request("http://localhost/api/portfolio?asOf=2026-05-20"),
        config(ws),
      );
      const body = await res.json();
      expect(res.status).toBe(200);
      expect(body.ok).toBe(true);
      expect(body.portfolio.companyCount).toBe(1);
      expect(body.portfolio.asOf).toBe("2026-05-20");
      expect(body.portfolio.companies[0].slug).toBe("acme-aps");
    } finally {
      cleanupDir(ws);
    }
  });
});
