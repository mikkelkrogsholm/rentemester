// Tests: src/server/data.ts — portfolio aggregation across workspace companies.
import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  buildCompanyAttention,
  buildCompanyOverview,
  buildPortfolioOverview,
} from "../../src/server/data";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import {
  companyRootForSlug,
  initWorkspace,
  loadWorkspaceManifest,
  saveWorkspaceManifest,
} from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry, verifyAuditChain } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { recordException } from "../../src/core/exceptions";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function dataDirectoryDigest(companyRoot: string) {
  const dataDir = join(companyRoot, "data");
  return readdirSync(dataDir).sort().map((name) => ({
    name,
    sha256: createHash("sha256").update(readFileSync(join(dataDir, name))).digest("hex"),
  }));
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
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("produces one summary per company", () => {
    const ws = tmpRoot("pf-summaries");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000001" });
      createCompany(ws, { name: "Beta IVS", cvr: "DK10000002" });
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
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("totals are the sum of per-company figures", () => {
    const ws = tmpRoot("pf-totals");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000003" });
      createCompany(ws, { name: "Beta IVS", cvr: "DK10000004" });
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const sum = overview.companies.reduce((a, c) => a + c.openInvoiceCount, 0);
      expect(overview.totals.openInvoiceCount).toBe(sum);
      const vatSum = overview.companies.reduce((a, c) => a + c.netVatPayable, 0);
      expect(overview.totals.netVatPayable).toBeCloseTo(vatSum, 6);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company carries its real current-year resultat, omsætning and VAT", () => {
    const ws = tmpRoot("pf-realfigures");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000005" });
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
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("VAT uses the booked position, not the old quarter report (0-bug fix)", () => {
    const ws = tmpRoot("pf-vatfix");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000006" });
      seedPnl(ws, "acme-aps", "2026-02-01", 1000, 0);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      // Output VAT booked on account 1200 (250) with no input → payable 250.
      expect(overview.companies[0]!.vat?.payable).toBe(250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("actual bank balance comes from the imported statement", () => {
    const ws = tmpRoot("pf-bank");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000007" });
      seedPnl(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankRow(ws, "acme-aps", "2026-04-01", 700, 700);
      seedBankRow(ws, "acme-aps", "2026-04-10", -200, 500);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.companies[0]!.actualBankBalance).toBe(500);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("open exceptions are grouped into Danish task lines", () => {
    const ws = tmpRoot("pf-tasks");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000008" });
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedException(ws, "acme-aps", "MAIL_INTAKE_NO_ATTACHMENT");
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      const c = overview.companies[0]!;
      expect(c.openTaskCount).toBe(buildCompanyAttention(ws, "acme-aps").count);
      expect(c.openTaskCount).toBeGreaterThanOrEqual(3);
      expect(c.openExceptionCount).toBe(3);
      const bank = c.taskGroups.find((g) => g.type === "UNMATCHED_BANK_TRANSACTION");
      expect(bank?.count).toBe(2);
      expect(bank?.label).toBe("2 banktransaktioner mangler afstemning");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("portfolio, company overview and inbox share the canonical attention count", () => {
    const ws = tmpRoot("pf-attention-parity");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Synthetic ApS", cvr: "DK10000018" });
      // No exception is recorded. The unmatched statement row instead makes
      // period-close readiness and the bookkeeping workbench actionable.
      seedBankRow(ws, "synthetic-aps", "2026-04-01", 500, 500);

      const inbox = buildCompanyAttention(ws, "synthetic-aps");
      const portfolio = buildPortfolioOverview(ws, "2026-05-20");
      const overview = buildCompanyOverview(ws, "synthetic-aps", 2026);

      expect(inbox.count).toBeGreaterThan(0);
      expect(overview.exceptions.count).toBe(0);
      expect(portfolio.companies[0]!.openTaskCount).toBe(inbox.count);
      expect(portfolio.companies[0]!.attentionStatus).toBe(inbox.status);
      expect(portfolio.rollup.openTaskCount).toBe(inbox.count);
      expect(overview.attention.count).toBe(inbox.count);
      expect(overview.attention.status).toBe(inbox.status);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("all attention surfaces stay clear for an empty canonical ledger", () => {
    const ws = tmpRoot("pf-attention-empty");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Empty ApS", cvr: "DK10000019" });
      const inbox = buildCompanyAttention(ws, "empty-aps");
      const portfolio = buildPortfolioOverview(ws, "2026-05-20");
      const overview = buildCompanyOverview(ws, "empty-aps", 2026);
      expect(inbox.count).toBe(0);
      expect(portfolio.companies[0]!.openTaskCount).toBe(0);
      expect(portfolio.companies[0]!.attentionStatus).toBe("clear");
      expect(portfolio.rollup.openTaskCount).toBe(0);
      expect(overview.attention.count).toBe(0);
      expect(overview.attention.status).toBe("clear");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("the roll-up sums resultat, liquidity, VAT and tasks across companies", () => {
    const ws = tmpRoot("pf-rollup");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000009" });
      createCompany(ws, { name: "Beta ApS", cvr: "DK10000010" });
      seedPnl(ws, "acme-aps", "2026-03-15", 1000, 400); // resultat 600, VAT 150
      seedBankRow(ws, "acme-aps", "2026-04-01", 500, 500);
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION");
      seedPnl(ws, "beta-aps", "2026-03-15", 2000, 500); // resultat 1500, VAT 375
      seedBankRow(ws, "beta-aps", "2026-04-01", 800, 800);
      const overview = buildPortfolioOverview(ws, "2026-05-20");
      expect(overview.rollup.resultat).toBe(2100);
      expect(overview.rollup.liquidity).toBe(1300);
      expect(overview.rollup.vatPayable).toBe(525);
      expect(overview.rollup.openTaskCount).toBe(
        overview.companies.reduce((sum, company) => sum + company.openTaskCount, 0),
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a registered live company with no ledger fails portfolio discovery closed", () => {
    const ws = tmpRoot("pf-noledger");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000011" });
      // Remove the ledger file to simulate a registered-but-missing company.
      rmSync(companyPaths(companyRootForSlug(ws, "acme-aps")).db, { force: true });
      expect(() => buildPortfolioOverview(ws, "2026-05-20")).toThrow(
        "WORKSPACE_LIVE_COMPANY_LEDGER_UNAVAILABLE:acme-aps",
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/portfolio surfaces the aggregation", async () => {
    const ws = tmpRoot("pf-endpoint");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Acme ApS", cvr: "DK10000012" });
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
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("portfolio discovery leaves ledger database and sidecars byte-for-byte unchanged", () => {
    const ws = tmpRoot("pf-read-only");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Read Only ApS", cvr: "DK10000017" });
      const companyRoot = companyRootForSlug(ws, "read-only-aps");
      const db = openDb(companyPaths(companyRoot).db);
      db.run("PRAGMA wal_checkpoint(TRUNCATE)");
      db.close();
      const before = dataDirectoryDigest(companyRoot);
      expect(buildPortfolioOverview(ws, "2026-05-20").companyCount).toBe(1);
      expect(dataDirectoryDigest(companyRoot)).toEqual(before);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("portfolio, dashboard and integrity agree on document-backed audit evidence", async () => {
    const ws = tmpRoot("pf-audit-evidence");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Evidence ApS", cvr: "DK10000018" });
      seedPnl(ws, "evidence-aps", "2026-03-15", 1000, 0);
      const companyRoot = companyRootForSlug(ws, "evidence-aps");
      const before = dataDirectoryDigest(companyRoot);

      const db = openDb(companyPaths(companyRoot).db);
      expect(verifyAuditChain(db, { companyRoot })).toMatchObject({ ok: true, errors: [] });
      db.close();

      const portfolio = await handleRequest(
        new Request("http://localhost/api/portfolio?asOf=2026-05-20"),
        config(ws),
      );
      const portfolioBody = await portfolio.json();
      expect(portfolioBody.portfolio.companies[0].auditChainOk).toBe(true);

      const dashboard = await handleRequest(
        new Request("http://localhost/api/companies/evidence-aps/dashboard?asOf=2026-05-20"),
        config(ws),
      );
      const dashboardBody = await dashboard.json();
      expect(dashboardBody.dashboard.audit.ok).toBe(true);

      const integrity = await handleRequest(
        new Request("http://localhost/api/companies/evidence-aps/integrity"),
        config(ws),
      );
      const integrityBody = await integrity.json();
      expect(integrityBody.integrity.auditChain).toMatchObject({ ok: true, errors: [] });
      expect(before.length).toBeGreaterThan(0);

      const original = readdirSync(join(companyRoot, "documents", "originals"))[0];
      if (!original) throw new Error("synthetic evidence file missing");
      rmSync(join(companyRoot, "documents", "originals", original));

      const brokenPortfolio = await handleRequest(
        new Request("http://localhost/api/portfolio?asOf=2026-05-20"),
        config(ws),
      );
      expect((await brokenPortfolio.json()).portfolio.companies[0].auditChainOk).toBe(false);
      const brokenIntegrity = await handleRequest(
        new Request("http://localhost/api/companies/evidence-aps/integrity"),
        config(ws),
      );
      expect((await brokenIntegrity.json()).integrity.auditChain.ok).toBe(false);
      const brokenDashboard = await handleRequest(
        new Request("http://localhost/api/companies/evidence-aps/dashboard?asOf=2026-05-20"),
        config(ws),
      );
      expect((await brokenDashboard.json()).dashboard.audit.ok).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("workspace reads ignore unregistered dry-run, baseline, restore and retest ledger copies", async () => {
    const ws = tmpRoot("pf-canonical-only");
    try {
      initWorkspace(ws);
      createCompany(ws, { name: "Canonical ApS", cvr: "DK10000013" });
      seedException(ws, "canonical-aps", "UNMATCHED_BANK_TRANSACTION");
      const canonicalRoot = companyRootForSlug(ws, "canonical-aps");
      for (const slug of ["dry-run-copy", "baseline-copy", "restore-copy", "retest-copy"]) {
        cpSync(canonicalRoot, join(ws, slug), { recursive: true });
      }
      const manifest = loadWorkspaceManifest(ws);
      saveWorkspaceManifest(ws, {
        ...manifest,
        companies: [...manifest.companies, {
          slug: "dry-run-copy",
          name: "Dry run copy",
          createdAt: "2026-05-20T00:00:00.000Z",
          archived: false,
          purpose: "dry-run",
        }],
      });
      const manifestBefore = readFileSync(join(ws, "workspace.json"));

      const listResponse = await handleRequest(new Request("http://localhost/api/companies"), config(ws));
      const listBody = await listResponse.json();
      expect(listResponse.status).toBe(200);
      expect(listBody.companies.map((company: { slug: string }) => company.slug)).toEqual(["canonical-aps"]);

      const portfolioResponse = await handleRequest(
        new Request("http://localhost/api/portfolio?asOf=2026-05-20"),
        config(ws),
      );
      const portfolioBody = await portfolioResponse.json();
      expect(portfolioResponse.status).toBe(200);
      expect(portfolioBody.portfolio.companyCount).toBe(1);
      expect(portfolioBody.portfolio.rollup.openTaskCount).toBe(
        buildCompanyAttention(ws, "canonical-aps").count,
      );

      const directResponse = await handleRequest(
        new Request("http://localhost/api/companies/dry-run-copy/dashboard"),
        config(ws),
      );
      expect(directResponse.status).toBe(404);
      expect(readFileSync(join(ws, "workspace.json"))).toEqual(manifestBefore);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("HTTP discovery fails closed on duplicate or missing live identity", async () => {
    const duplicateWs = tmpRoot("pf-duplicate-cvr");
    const missingWs = tmpRoot("pf-missing-cvr");
    try {
      initWorkspace(duplicateWs);
      createCompany(duplicateWs, { name: "Alpha ApS", cvr: "DK10000014" });
      createCompany(duplicateWs, { name: "Beta ApS", cvr: "10000014" });
      for (const path of ["/api/companies", "/api/portfolio?asOf=2026-05-20"]) {
        const response = await handleRequest(new Request(`http://localhost${path}`), config(duplicateWs));
        const body = await response.json();
        expect(response.status).toBe(409);
        expect(body.subcode).toBe("WORKSPACE_CANONICALITY_FAILED");
        expect(body.errors).toEqual([
          "WORKSPACE_DUPLICATE_LEGAL_IDENTITY:alpha-aps,WORKSPACE_DUPLICATE_LEGAL_IDENTITY:beta-aps",
        ]);
        expect(JSON.stringify(body)).not.toContain("10000014");
        expect(body).not.toHaveProperty("portfolio");
        expect(body).not.toHaveProperty("companies");
      }

      initWorkspace(missingWs);
      createCompany(missingWs, { name: "No Identity ApS" });
      const missing = await handleRequest(new Request("http://localhost/api/companies"), config(missingWs));
      expect(missing.status).toBe(200);
      expect(await missing.json()).toMatchObject({ count: 0, companies: [] });
    } finally {
      rmSync(duplicateWs, { recursive: true, force: true });
      rmSync(missingWs, { recursive: true, force: true });
    }
  });
});
