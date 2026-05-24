// Tests: src/server/router.ts, src/server/auth.ts, src/server/errors.ts,
// src/server/config.ts — endpoint contracts, the auth seam, and safe errors.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { resolveServerConfig, type ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import {
  initWorkspace,
  companyRootForSlug,
  loadWorkspaceManifest,
  saveWorkspaceManifest,
} from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { issueInvoice } from "../../src/core/issued-invoices";
import { createCustomer, createVendor } from "../../src/core/master-data";
import { recordException } from "../../src/core/exceptions";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

/** A workspace with the named companies created in it. */
function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

function config(overrides: Partial<ServerConfig> & { workspaceRoot: string }): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    ...overrides,
  };
}

async function get(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  const body = await res.json();
  return { status: res.status, body };
}

describe("cockpit API — config", () => {
  test("defaults to the localhost bind address", () => {
    const cfg = resolveServerConfig({
      workspaceRoot: "/tmp/ws",
      env: {},
    });
    expect(cfg.host).toBe("127.0.0.1");
    expect(cfg.port).toBe(4319);
    expect(cfg.authRequired).toBe(false);
  });

  test("bind address is config-driven via env", () => {
    const cfg = resolveServerConfig({
      workspaceRoot: "/tmp/ws",
      env: { RENTEMESTER_APP_HOST: "0.0.0.0", RENTEMESTER_APP_PORT: "9000" },
    });
    expect(cfg.host).toBe("0.0.0.0");
    expect(cfg.port).toBe(9000);
  });

  test("rejects a non-numeric port", () => {
    expect(() =>
      resolveServerConfig({ workspaceRoot: "/tmp/ws", env: { RENTEMESTER_APP_PORT: "abc" } }),
    ).toThrow(/RENTEMESTER_APP_PORT/);
  });

  test("requires a workspace root", () => {
    expect(() => resolveServerConfig({ env: {} })).toThrow(/workspace/);
  });
});

describe("cockpit API — auth seam", () => {
  test("phase 1 (localhost-trusted) is a pass-through", async () => {
    const ws = makeWorkspace("auth-passthrough");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/health");
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("when auth is enabled the seam rejects an unauthenticated request", async () => {
    const ws = makeWorkspace("auth-reject");
    try {
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await get(cfg, "/api/health");
      expect(res.status).toBe(401);
      expect(res.body.ok).toBe(false);
      expect(res.body.error.code).toBe("unauthorized");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("when auth is enabled a valid bearer token passes the seam", async () => {
    const ws = makeWorkspace("auth-accept");
    try {
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await get(cfg, "/api/health", {
        headers: { authorization: "Bearer s3cret" },
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid bearer token is rejected", async () => {
    const ws = makeWorkspace("auth-badtoken");
    try {
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await get(cfg, "/api/health", {
        headers: { authorization: "Bearer wrong" },
      });
      expect(res.status).toBe(401);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — endpoint contracts", () => {
  test("GET /api/health reports the service", async () => {
    const ws = makeWorkspace("ep-health");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/health");
      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({ ok: true, service: "rentemester-cockpit" });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies lists workspace companies", async () => {
    const ws = makeWorkspace("ep-companies", ["Acme ApS", "Beta IVS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies");
      expect(res.status).toBe(200);
      expect(res.body.count).toBe(2);
      const slugs = res.body.companies.map((c: any) => c.slug).sort();
      expect(slugs).toEqual(["acme-aps", "beta-ivs"]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies discovers a populated but unlisted company dir (#256)", async () => {
    // An owner set the company up via the CLI: its directory + ledger sit in
    // the workspace but the cockpit manifest never recorded it. Pre-#256 the
    // cockpit showed "0 virksomheder" and a create-company would mint an empty
    // ledger over it. The cockpit must instead discover and adopt it.
    const ws = makeWorkspace("ep-discover", ["Acme ApS"]);
    try {
      // Drop the company from the manifest, leaving the directory + ledger.
      const manifest = loadWorkspaceManifest(ws);
      saveWorkspaceManifest(ws, { ...manifest, companies: [] });
      // Before discovery the manifest is empty…
      expect(loadWorkspaceManifest(ws).companies).toHaveLength(0);

      const res = await get(config({ workspaceRoot: ws }), "/api/companies");
      expect(res.status).toBe(200);
      // …yet the cockpit surfaces the real company, not "0 virksomheder".
      expect(res.body.count).toBe(1);
      expect(res.body.companies[0].slug).toBe("acme-aps");
      expect(res.body.companies[0].name).toBe("Acme ApS");
      // The discovery is persisted: the manifest now records the company.
      expect(loadWorkspaceManifest(ws).companies.map((c) => c.slug)).toEqual([
        "acme-aps",
      ]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/portfolio discovers an unlisted company dir (#256)", async () => {
    // The portfolio is the cockpit's landing page — an owner who set a company
    // up via the CLI must land on it, not on the empty-workspace onboarding.
    const ws = makeWorkspace("ep-discover-pf", ["Acme ApS"]);
    try {
      const manifest = loadWorkspaceManifest(ws);
      saveWorkspaceManifest(ws, { ...manifest, companies: [] });
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio");
      expect(res.status).toBe(200);
      expect(res.body.portfolio.companyCount).toBe(1);
      expect(res.body.portfolio.companies[0].slug).toBe("acme-aps");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies/:slug/dashboard returns dashboard data", async () => {
    const ws = makeWorkspace("ep-dashboard", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/acme-aps/dashboard");
      expect(res.status).toBe(200);
      expect(res.body.dashboard.slug).toBe("acme-aps");
      expect(res.body.dashboard.company.name).toBe("Acme ApS");
      expect(res.body.dashboard.invoices.count).toBe(0);
      expect(res.body.dashboard).toHaveProperty("vat");
      expect(res.body.dashboard).toHaveProperty("audit");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET dashboard for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("ep-dashboard-404", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/ghost/dashboard");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown endpoint is a safe 404", async () => {
    const ws = makeWorkspace("ep-unknown");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/nope");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a wrong method on a known route is 405", async () => {
    const ws = makeWorkspace("ep-405");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio", {
        method: "DELETE",
      });
      expect(res.status).toBe(405);
      expect(res.body.error.code).toBe("method_not_allowed");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid asOf query value is rejected with a safe 400", async () => {
    const ws = makeWorkspace("ep-badasof");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio?asOf=not-a-date");
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("error responses never leak a filesystem path", async () => {
    const ws = makeWorkspace("ep-noleak", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/ghost/dashboard");
      expect(JSON.stringify(res.body)).not.toContain(ws);
      expect(JSON.stringify(res.body)).not.toContain("/");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/** Posts one balanced entry into a workspace company's ledger. */
function postEntry(ws: string, slug: string, transactionDate: string) {
  const dbPath = companyPaths(companyRootForSlug(ws, slug)).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    // Two asset accounts keep the entry document-free (no income/expense line).
    const res = postJournalEntry(db, {
      transactionDate,
      text: "Test posting",
      lines: [
        { accountNo: "1100", debitAmount: 100 },
        { accountNo: "2000", creditAmount: 100 },
      ],
    });
    if (!res.ok) throw new Error(res.errors.join("; "));
  } finally {
    db.close();
  }
}

describe("cockpit API — fiscal years (GET /api/companies/:slug/fiscal-years)", () => {
  test("an empty ledger has no fiscal years", async () => {
    const ws = makeWorkspace("fy-empty", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/acme-aps/fiscal-years");
      expect(res.status).toBe(200);
      expect(res.body.fiscalYears.slug).toBe("acme-aps");
      expect(res.body.fiscalYears.years).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a posted entry surfaces its fiscal year as a live year", async () => {
    const ws = makeWorkspace("fy-live", ["Acme ApS"]);
    try {
      postEntry(ws, "acme-aps", "2026-03-15");
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/acme-aps/fiscal-years");
      expect(res.status).toBe(200);
      expect(res.body.fiscalYears.years).toEqual([
        { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" },
      ]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("multiple years are returned newest-first", async () => {
    const ws = makeWorkspace("fy-multi", ["Acme ApS"]);
    try {
      postEntry(ws, "acme-aps", "2025-06-01");
      postEntry(ws, "acme-aps", "2026-02-01");
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/acme-aps/fiscal-years");
      expect(res.body.fiscalYears.years.map((y: any) => y.label)).toEqual(["2026", "2025"]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("fiscal-years for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("fy-404", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/ghost/fiscal-years");
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/**
 * Posts a profit-and-loss pair into a workspace company's ledger: a sale that
 * credits income account `1000` (VAT code `DK_SALE_25`) and a purchase that
 * debits expense account `3000` (VAT code `DK_PURCHASE_25`). Both are 25%-VAT
 * standard-rated, so the overview's VAT block is exercised. The cash side runs
 * over bank account `2000`. Income/expense lines require a document, so a
 * minimal one is ingested first.
 */
function postPnlEntry(
  ws: string,
  slug: string,
  transactionDate: string,
  income: number,
  expense: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const dbPath = companyPaths(companyRoot).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-ov-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    // Date-unique content so the helper can be called for several years in
    // one test without colliding on the document content-hash dedupe.
    writeFileSync(sourceFile, `Bilag ${transactionDate}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: transactionDate,
      invoiceNo: `OV-${transactionDate}`,
      deliveryDescription: "Overblik testbilag",
      amountIncVat: 1,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Acme ApS", address: "Vej 2", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bankoverførsel",
    });
    if (!doc.ok) throw new Error("doc ingest failed: " + (doc.errors ?? []).join("; "));

    if (income > 0) {
      const sale = postJournalEntry(db, {
        transactionDate,
        text: "Overblik salg",
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
        transactionDate,
        text: "Overblik køb",
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

/**
 * Posts a bad-debt (debitortab) write-off journal entry — the same shape
 * `core/invoice-bad-debt.ts` books: a `DK_BAD_DEBT_25` loss-basis debit, a
 * debit on the output-VAT account `1200` to claim the VAT relief, and a credit
 * writing off the receivable. `net` is the loss base; the VAT relief is 25% of
 * it. Used to reproduce the cockpit VAT bugs (#271, #272).
 */
function postBadDebtWriteoff(
  ws: string,
  slug: string,
  transactionDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-baddebt-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    writeFileSync(sourceFile, `Debitortab ${transactionDate}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: transactionDate,
      invoiceNo: `BD-${transactionDate}`,
      deliveryDescription: "Debitortab testbilag",
      amountIncVat: 1,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Acme ApS", address: "Vej 2", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bankoverførsel",
    });
    if (!doc.ok) throw new Error("doc ingest failed: " + (doc.errors ?? []).join("; "));
    const vat = net * 0.25;
    const res = postJournalEntry(db, {
      transactionDate,
      text: "Tab på debitor — bad debt write-off",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3080", debitAmount: net, vatCode: "DK_BAD_DEBT_25" },
        { accountNo: "1200", debitAmount: vat },
        { accountNo: "1100", creditAmount: net + vat },
      ],
    });
    if (!res.ok) throw new Error("bad-debt post failed: " + res.errors.join("; "));
  } finally {
    db.close();
  }
}

describe("cockpit API — overview (GET /api/companies/:slug/overview)", () => {
  test("returns the P&L, bank and VAT blocks for the live year", async () => {
    const ws = makeWorkspace("ov-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.overview.slug).toBe("acme-aps");
      expect(res.body.overview.selectedYear).toBe("2026");
      expect(res.body.overview.archived).toBe(false);
      expect(res.body.overview.profitAndLoss.omsaetning).toBe(1000);
      expect(res.body.overview.profitAndLoss.udgifter).toBe(400);
      expect(res.body.overview.profitAndLoss.resultat).toBe(600);
      expect(res.body.overview.profitAndLoss.months).toHaveLength(12);
      expect(res.body.overview.profitAndLoss.months[2].income).toBe(1000);
      expect(res.body.overview.profitAndLoss.months[2].expense).toBe(400);
      // VAT: 25% of the 1000 sales base / the 400 purchase base. The P&L
      // entry is dated 2026-03-15 (Q1), so the surfaced quarter is Q1 2026 —
      // quarterly is the only VAT cadence, consistent with the dashboard/CLI.
      expect(res.body.overview.vat.outputVat).toBe(250);
      expect(res.body.overview.vat.inputVat).toBe(100);
      expect(res.body.overview.vat.payable).toBe(150);
      expect(res.body.overview.vat.periodLabel).toBe("Q1 2026");
      // Bank account 2000 nets +1250 (sale) −500 (purchase) = 750.
      expect(res.body.overview.bank.balance).toBe(750);
      expect(res.body.overview.recentEntries.length).toBeGreaterThan(0);
      expect(res.body.overview.fiscalYears[0].label).toBe("2026");
      // "Senest bogført" — the most recent posted transaction date.
      expect(res.body.overview.lastPostedDate).toBe("2026-03-15");
      // Nøgletal: bruttomargin = resultat ÷ omsætning = 600/1000 = 0.6.
      expect(res.body.overview.keyFigures.bruttomargin).toBeCloseTo(0.6, 6);
      // Egenkapitalandel is a fraction (0–1) when the balance has assets.
      expect(
        typeof res.body.overview.keyFigures.egenkapitalandel,
      ).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("defaults to the most recent live year when year is omitted", async () => {
    const ws = makeWorkspace("ov-default", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-02-01", 500, 100);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview",
      );
      expect(res.status).toBe(200);
      expect(res.body.overview.selectedYear).toBe("2026");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid year query value is a safe 400", async () => {
    const ws = makeWorkspace("ov-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("ov-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/overview",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("bank block reports actual balance and the gap to the booked balance", async () => {
    const ws = makeWorkspace("ov-bank-actual", ["Acme ApS"]);
    try {
      // Booked balance on account 2000 nets +1250 (sale) −500 (purchase) = 750.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      // Statement closes at 500 — short of the booked 750 by 250.
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Indbetaling", 700, 700);
      seedBankTransaction(ws, "acme-aps", "2026-04-10", "Gebyr", -200, 500);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const bank = res.body.overview.bank;
      expect(bank.balance).toBe(750);
      expect(bank.actualBalance).toBe(500);
      expect(bank.difference).toBe(250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("groups same-type exceptions into one Danish summary line", async () => {
    const ws = makeWorkspace("ov-exc-group", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 1 unmatched");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 2 unmatched");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 3 unmatched");
      seedException(ws, "acme-aps", "MAIL_INTAKE_NO_ATTACHMENT", "Mail without attachment");
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const exc = res.body.overview.exceptions;
      expect(exc.count).toBe(4);
      // Two groups: 3 bank rows + 1 mail row, each one line.
      expect(exc.groups.length).toBe(2);
      const bankGroup = exc.groups.find(
        (g: { type: string }) => g.type === "UNMATCHED_BANK_TRANSACTION",
      );
      expect(bankGroup.count).toBe(3);
      expect(bankGroup.label).toContain("3 banktransaktioner");
      expect(bankGroup.link).toBe("bank");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("each overview exception row carries its requiredAction guidance (#254)", async () => {
    const ws = makeWorkspace("ov-exc-action", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedException(
        ws,
        "acme-aps",
        "UNMATCHED_BANK_TRANSACTION",
        "Banktransaktion 12 mangler afstemning",
        "Find bilaget for indbetalingen og bogfør den som indtægt.",
      );
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const row = res.body.overview.exceptions.rows[0];
      expect(row.message).toBe("Banktransaktion 12 mangler afstemning");
      // The concrete action — the most useful part — is on the wire (#254).
      expect(row.requiredAction).toBe(
        "Find bilaget for indbetalingen og bogfør den som indtægt.",
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — income statement (GET .../income-statement)", () => {
  test("returns grouped income/expense lines and the result for the year", async () => {
    const ws = makeWorkspace("is-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2026",
      );
      expect(res.status).toBe(200);
      const is = res.body.incomeStatement;
      expect(is.slug).toBe("acme-aps");
      expect(is.selectedYear).toBe("2026");
      expect(is.archived).toBe(false);
      expect(is.totalIncome).toBe(1000);
      expect(is.totalExpense).toBe(400);
      expect(is.result).toBe(600);
      expect(is.income[0]).toMatchObject({ amount: 1000, priorAmount: 0 });
      expect(is.expense[0]).toMatchObject({ amount: 400, priorAmount: 0 });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a prior-year posting surfaces as the comparison amount", async () => {
    const ws = makeWorkspace("is-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2025-04-01", 800, 0);
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 0);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.incomeStatement.income[0].amount).toBe(1000);
      expect(res.body.incomeStatement.income[0].priorAmount).toBe(800);
      expect(res.body.incomeStatement.priorTotalIncome).toBe(800);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("income-statement for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("is-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/income-statement",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — balance sheet (GET .../balance)", () => {
  test("returns asset/liability/equity sections that balance", async () => {
    const ws = makeWorkspace("bal-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.slug).toBe("acme-aps");
      expect(b.asOfDate).toBe("2026-12-31");
      expect(b.balanced).toBe(true);
      expect(b.totalAssets).toBe(b.totalLiabilitiesAndEquity);
      expect(b.assets).toHaveProperty("lines");
      expect(b.assets).toHaveProperty("total");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("balance for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("bal-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/balance",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a prior-year posting surfaces a priorAmount on each balance line and a priorTotal on each section (#400)", async () => {
    const ws = makeWorkspace("bal-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2025-04-01", 800, 0);
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      // Every section carries a priorTotal — a number, not null — because a
      // prior live year exists in the ledger.
      expect(typeof b.assets.priorTotal).toBe("number");
      expect(typeof b.liabilities.priorTotal).toBe("number");
      expect(typeof b.equity.priorTotal).toBe("number");
      expect(typeof b.priorTotalLiabilitiesAndEquity).toBe("number");
      // The prior balance balances by definition (double-entry).
      expect(b.assets.priorTotal).toBeCloseTo(
        b.priorTotalLiabilitiesAndEquity,
        2,
      );
      // The synthetic "Årets resultat" line in equity carries last year's
      // result (800) as its priorAmount, not the current year's.
      const aretsResultat = b.equity.lines.find(
        (l: { name: string }) => l.name === "Årets resultat",
      );
      expect(aretsResultat).toBeTruthy();
      expect(aretsResultat.priorAmount).toBeCloseTo(800, 2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("when no prior year exists, every priorAmount and priorTotal is null (#400)", async () => {
    // A company in its first regnskabsår — the ledger has no foregående år,
    // so the prior column must be uniformly null rather than misleadingly 0.
    const ws = makeWorkspace("bal-no-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.assets.priorTotal).toBeNull();
      expect(b.liabilities.priorTotal).toBeNull();
      expect(b.equity.priorTotal).toBeNull();
      expect(b.priorTotalLiabilitiesAndEquity).toBeNull();
      for (const l of b.assets.lines) expect(l.priorAmount).toBeNull();
      for (const l of b.liabilities.lines) expect(l.priorAmount).toBeNull();
      for (const l of b.equity.lines) expect(l.priorAmount).toBeNull();
      // The balanced-flag only covers the current year — see acceptkriterier.
      expect(b.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — trial balance (GET .../trial-balance)", () => {
  test("lists every moved account with debit, credit and balance", async () => {
    const ws = makeWorkspace("tb-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=2026",
      );
      expect(res.status).toBe(200);
      const tb = res.body.trialBalance;
      expect(tb.slug).toBe("acme-aps");
      expect(tb.balanced).toBe(true);
      expect(tb.totalDebit).toBe(tb.totalCredit);
      expect(tb.rows.length).toBeGreaterThan(0);
      expect(tb.rows[0]).toHaveProperty("accountNo");
      expect(tb.rows[0]).toHaveProperty("debit");
      expect(tb.rows[0]).toHaveProperty("credit");
      expect(tb.rows[0]).toHaveProperty("balance");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid year query value is a safe 400", async () => {
    const ws = makeWorkspace("tb-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("trial-balance for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("tb-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/trial-balance",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — journal (GET .../journal)", () => {
  test("returns posted entries for the year, each with its lines", async () => {
    const ws = makeWorkspace("jrn-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2026",
      );
      expect(res.status).toBe(200);
      const j = res.body.journal;
      expect(j.slug).toBe("acme-aps");
      expect(j.archived).toBe(false);
      expect(j.entries.length).toBe(2);
      const entry = j.entries[0];
      expect(entry).toHaveProperty("entryNo");
      expect(entry).toHaveProperty("total");
      expect(entry.lines.length).toBeGreaterThan(0);
      expect(entry.lines[0]).toHaveProperty("accountNo");
      expect(entry.lines[0]).toHaveProperty("accountName");
      expect(entry.lines[0]).toHaveProperty("debit");
      expect(entry.lines[0]).toHaveProperty("credit");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an ?account= filter limits the journal to that account's entries", async () => {
    const ws = makeWorkspace("jrn-acct", ["Acme ApS"]);
    try {
      // A sale (touches 1000/1200/2000) and a purchase (3000/4000/2000).
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const cfg = config({ workspaceRoot: ws });

      // Account 1000 only appears on the sale — exactly one entry.
      const sale = await get(
        cfg,
        "/api/companies/acme-aps/journal?year=2026&account=1000",
      );
      expect(sale.status).toBe(200);
      expect(sale.body.journal.accountFilter.accountNo).toBe("1000");
      expect(sale.body.journal.entries.length).toBe(1);
      expect(sale.body.journal.entries[0].text).toBe("Overblik salg");

      // Account 2000 (bank) is on both — two entries.
      const bank = await get(
        cfg,
        "/api/companies/acme-aps/journal?year=2026&account=2000",
      );
      expect(bank.body.journal.entries.length).toBe(2);

      // Without the filter, accountFilter is null and all entries are shown.
      const all = await get(cfg, "/api/companies/acme-aps/journal?year=2026");
      expect(all.body.journal.accountFilter).toBeNull();
      expect(all.body.journal.entries.length).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("#379 — each entry carries its linked documentId and documentNo so the cockpit can link from posting to bilag", async () => {
    const ws = makeWorkspace("jrn-doc", ["Acme ApS"]);
    try {
      // postPnlEntry ingester et bilag og bogfører posten med `documentId`
      // sat, så journal-endpointet skal returnere `documentId` !== null og
      // `documentNo` matching `OV-<dato>`.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2026",
      );
      expect(res.status).toBe(200);
      const entries = res.body.journal.entries as Array<{
        documentId: number | null;
        documentNo: string | null;
        text: string;
      }>;
      expect(entries.length).toBe(2);
      for (const entry of entries) {
        expect(entry).toHaveProperty("documentId");
        expect(entry).toHaveProperty("documentNo");
        // postPnlEntry sætter documentId på begge posts.
        expect(entry.documentId).not.toBeNull();
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("journal for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("jrn-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/journal",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// Archive-aware core views (#197 — Runde 3, iteration 10)
//
// The core statement endpoints derive their figures from `import_archive_*`
// when the selected year is an archived one — the same chart of accounts
// classification the live ledger uses, applied to the archived SaldoBalance.
// --------------------------------------------------------------------------

describe("cockpit API — archive-aware core views (#197)", () => {
  test("income-statement classifies an archived year's SaldoBalance", async () => {
    const ws = makeWorkspace("arc-is", ["Acme ApS"]);
    try {
      // Archived 2024 — income 1000 closes at −5000 (credit), expense 3000 at
      // 1200 (debit). Resultat = 5000 − 1200 = 3800.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 1200],
        ["2000", "Bank", 3800],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2024",
      );
      expect(res.status).toBe(200);
      const is = res.body.incomeStatement;
      expect(is.archived).toBe(true);
      expect(is.archivedSource).toBe("dinero");
      expect(is.income).toHaveLength(1);
      expect(is.income[0].amount).toBe(5000);
      expect(is.expense).toHaveLength(1);
      expect(is.expense[0].amount).toBe(1200);
      expect(is.totalIncome).toBe(5000);
      expect(is.totalExpense).toBe(1200);
      expect(is.result).toBe(3800);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("balance classifies an archived year into asset/liability/equity", async () => {
    const ws = makeWorkspace("arc-bal", ["Acme ApS"]);
    try {
      // Assets 6000 debit (2000), liability 4500 credit (−1000 archive sign),
      // equity 5000 credit (−2000), income/expense net to the 3000 result.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["2000", "Bank", 6000],
        ["4500", "Momsafregning", -1000],
        ["5000", "Egenkapital", -2000],
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 2000],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2024",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.archived).toBe(true);
      expect(b.totalAssets).toBe(6000);
      expect(b.liabilities.total).toBe(1000);
      // The 3000 period result is folded into equity as an "Årets resultat"
      // line, so equity.total is the equity-account sum (2000) plus the result.
      expect(b.periodResult).toBe(3000);
      const resultLine = b.equity.lines.find(
        (l: { name: string }) => l.name === "Årets resultat",
      );
      expect(resultLine?.amount).toBe(3000);
      expect(b.equity.total).toBe(5000);
      // The archived balance sheet balances: assets = liabilities + equity.
      expect(b.totalLiabilitiesAndEquity).toBe(6000);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);
      expect(b.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archived equity.total matches the Flerårsoversigt's egenkapital", async () => {
    const ws = makeWorkspace("arc-bal-consistency", ["Acme ApS"]);
    try {
      // A distressed year: a negative (overdrawn) bank, an equity deficit and
      // a loss. The archived SaldoBalance is debit-signed and sums to zero, so
      // the sheet must still balance and the two views must agree on equity.
      seedArchiveYear(ws, "acme-aps", 2023, [
        ["2000", "Bank", -3000],
        ["4500", "Momsafregning", -500],
        ["5000", "Egenkapital", 1500],
        ["1000", "Omsætning", -2000],
        ["3000", "Software", 4000],
      ]);
      const cfg = config({ workspaceRoot: ws });
      const balRes = await get(cfg, "/api/companies/acme-aps/balance?year=2023");
      expect(balRes.status).toBe(200);
      const b = balRes.body.balance;
      // The balance balances even for a distressed (negative-asset) year.
      expect(b.balanced).toBe(true);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);

      const myRes = await get(cfg, "/api/companies/acme-aps/multi-year");
      expect(myRes.status).toBe(200);
      const my2023 = myRes.body.multiYear.years.find(
        (r: { year: string }) => r.year === "2023",
      );
      // The Balance view and the Flerårsoversigt agree on equity.
      expect(my2023.egenkapital).toBe(b.equity.total);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archived vat accounts are classified by normal balance, consistently across Balance and Flerårsoversigt (#321)", async () => {
    const ws = makeWorkspace("arc-vat-class", ["Acme ApS"]);
    try {
      // The native-Rentemester chart types `4000` Købsmoms as `vat`/debit
      // (input VAT — a receivable, so an asset) and `1200` Salgsmoms as
      // `vat`/credit (output VAT — a payable, so a liability). An archived
      // SaldoBalance carrying both must place them by their normal balance:
      // `4000` under assets, `1200` under liabilities. The shared #321
      // classification guarantees the Balance view and the Flerårsoversigt
      // agree — before #321 the Flerårsoversigt left `vat` accounts
      // unclassified, so its `balancesum` silently dropped the `4000` asset.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["2000", "Bank", 5000],
        ["4000", "Købsmoms", 1000], // vat/debit → an asset
        ["1200", "Salgsmoms", -2000], // vat/credit → a liability
        ["5000", "Egenkapital", -1000],
        ["1000", "Omsætning", -6000],
        ["3000", "Software", 3000],
      ]);
      const cfg = config({ workspaceRoot: ws });

      const balRes = await get(cfg, "/api/companies/acme-aps/balance?year=2024");
      expect(balRes.status).toBe(200);
      const b = balRes.body.balance;
      // The vat/debit Købsmoms is an asset: 5000 Bank + 1000 Købsmoms.
      expect(b.totalAssets).toBe(6000);
      // The vat/credit Salgsmoms is a liability: 2000.
      expect(b.liabilities.total).toBe(2000);
      // The sheet still balances: assets = liabilities + equity.
      expect(b.balanced).toBe(true);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);

      const myRes = await get(cfg, "/api/companies/acme-aps/multi-year");
      expect(myRes.status).toBe(200);
      const my2024 = myRes.body.multiYear.years.find(
        (r: { year: string }) => r.year === "2024",
      );
      // The Flerårsoversigt's balancesum counts the vat/debit account as an
      // asset, exactly as the Balance view does — the two never disagree.
      expect(my2024.balancesum).toBe(b.totalAssets);
      expect(my2024.egenkapital).toBe(b.equity.total);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("trial-balance renders the archived SaldoBalance directly", async () => {
    const ws = makeWorkspace("arc-tb", ["Acme ApS"]);
    try {
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 1200],
        ["2000", "Bank", 3800],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=2024",
      );
      expect(res.status).toBe(200);
      const t = res.body.trialBalance;
      expect(t.archived).toBe(true);
      expect(t.rows).toHaveLength(3);
      const income = t.rows.find((r: any) => r.accountNo === "1000");
      expect(income.credit).toBe(5000);
      expect(income.debit).toBe(0);
      expect(t.totalDebit).toBe(5000);
      expect(t.totalCredit).toBe(5000);
      expect(t.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("journal groups archived postings by voucher", async () => {
    const ws = makeWorkspace("arc-jrn", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [["1000", "Omsætning", -5000]],
        [],
      );
      // Two postings share voucher "B-1"; one carries voucher "B-2".
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        const yearId = (
          db
            .query(
              "SELECT id FROM import_archive_years WHERE fiscal_year = 2024",
            )
            .get() as { id: number }
        ).id;
        const ins = db.prepare(
          `INSERT INTO import_archive_postings
             (archive_year_id, line_no, account_no, account_name,
              transaction_date, voucher, text, amount)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
        );
        ins.run(yearId, 0, "1000", "Omsætning", "2024-03-01", "B-1", "Salg", -5000);
        ins.run(yearId, 1, "2000", "Bank", "2024-03-01", "B-1", "Salg", 5000);
        ins.run(yearId, 2, "3000", "Software", "2024-06-01", "B-2", "Køb", 1200);
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2024",
      );
      expect(res.status).toBe(200);
      const j = res.body.journal;
      expect(j.archived).toBe(true);
      expect(j.entries).toHaveLength(2);
      // Newest first — B-2 (June) before B-1 (March).
      expect(j.entries[0].entryNo).toBe("B-2");
      const b1 = j.entries.find((e: any) => e.entryNo === "B-1");
      expect(b1.lines).toHaveLength(2);
      expect(b1.total).toBe(5000);
      // The ?account= drill-down filters archived entries too.
      const filtered = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2024&account=3000",
      );
      expect(filtered.body.journal.entries).toHaveLength(1);
      expect(filtered.body.journal.accountFilter.accountNo).toBe("3000");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview derives a P&L overview for an archived year", async () => {
    const ws = makeWorkspace("arc-ov", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [
          ["1000", "Omsætning", -5000],
          ["3000", "Software", 1200],
        ],
        [],
      );
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        const yearId = (
          db
            .query(
              "SELECT id FROM import_archive_years WHERE fiscal_year = 2024",
            )
            .get() as { id: number }
        ).id;
        const ins = db.prepare(
          `INSERT INTO import_archive_postings
             (archive_year_id, line_no, account_no, account_name,
              transaction_date, voucher, text, amount)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
        );
        ins.run(yearId, 0, "1000", "Omsætning", "2024-03-10", "B-1", "Salg", -5000);
        ins.run(yearId, 1, "3000", "Software", "2024-03-10", "B-1", "Køb", 1200);
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2024",
      );
      expect(res.status).toBe(200);
      const o = res.body.overview;
      expect(o.archived).toBe(true);
      expect(o.archivedSource).toBe("dinero");
      expect(o.profitAndLoss.omsaetning).toBe(5000);
      expect(o.profitAndLoss.udgifter).toBe(1200);
      expect(o.profitAndLoss.resultat).toBe(3800);
      expect(o.profitAndLoss.months).toHaveLength(12);
      // March (index 2) carries the bucketed activity.
      expect(o.profitAndLoss.months[2].income).toBe(5000);
      expect(o.profitAndLoss.months[2].expense).toBe(1200);
      // Live-only sections are honestly N/A, not faked.
      expect(o.vat).toBeNull();
      expect(o.bank.actualBalance).toBeNull();
      expect(o.recentEntries.length).toBeGreaterThan(0);
      expect(o.lastPostedDate).toBe("2024-03-10");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/**
 * Inserts an imported bank transaction directly into a company's ledger.
 * When `balanceAfter` is given the import's running balance is recorded — the
 * statement figure the actual-balance helper reads.
 */
function seedBankTransaction(
  ws: string,
  slug: string,
  transactionDate: string,
  text: string,
  amount: number,
  balanceAfter?: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    db.query(
      `INSERT INTO bank_transactions
         (transaction_date, text, amount, currency, transaction_hash, status,
          balance_after)
       VALUES (?, ?, ?, 'DKK', ?, 'imported', ?)`,
    ).run(
      transactionDate,
      text,
      amount,
      `hash-${transactionDate}-${text}`,
      balanceAfter ?? null,
    );
  } finally {
    db.close();
  }
}

/** Records an open exception of the given type directly in a company's ledger. */
function seedException(
  ws: string,
  slug: string,
  type: string,
  message: string,
  requiredAction?: string,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    recordException(db, {
      type,
      severity: "medium",
      message,
      ...(requiredAction ? { requiredAction } : {}),
    });
  } finally {
    db.close();
  }
}

describe("cockpit API — bank (GET .../bank)", () => {
  test("returns transactions with reconciliation status and booked balance", async () => {
    const ws = makeWorkspace("bnk-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Bankgebyr", -50);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.slug).toBe("acme-aps");
      expect(b.transactions.length).toBe(1);
      expect(b.transactions[0].reconciliationStatus).toBe("unmatched");
      expect(b.unmatchedCount).toBe(1);
      expect(b).toHaveProperty("bookedBalance");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("reports the actual statement balance and the gap to booked", async () => {
    const ws = makeWorkspace("bnk-actual", ["Acme ApS"]);
    try {
      // Booked balance on account 2000 = 750 (see postPnlEntry).
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Indbetaling", 700, 700);
      seedBankTransaction(ws, "acme-aps", "2026-04-10", "Gebyr", -200, 500);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.bookedBalance).toBe(750);
      expect(b.actualBalance).toBe(500);
      expect(b.difference).toBe(250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("bank for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("bnk-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/bank",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("shows the imported bank transactions for an archived fiscal year", async () => {
    const ws = makeWorkspace("bnk-archived", ["Acme ApS"]);
    try {
      // 2024 is an archived year from a prior-system migration; the live
      // ledger's only year is 2026. But the owner's bank-statement CSV spans
      // both years — its 2024 rows are live, append-only data and must still
      // be shown when the owner selects the archived year.
      seedArchiveYear(ws, "acme-aps", 2024, [["2000", "Bank", 3800]]);
      seedBankTransaction(ws, "acme-aps", "2024-06-01", "Leverandørbetaling", -500, 4200);
      seedBankTransaction(ws, "acme-aps", "2024-09-15", "Kundeindbetaling", 1200, 5400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2024",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.archived).toBe(true);
      expect(b.transactions.length).toBe(2);
      expect(b.transactions.map((t: { text: string }) => t.text)).toEqual([
        "Leverandørbetaling",
        "Kundeindbetaling",
      ]);
      // The statement's own running balance is valid for an archived year.
      expect(b.actualBalance).toBe(5400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — VAT (GET .../vat)", () => {
  test("returns the output/input/payable VAT for the period", async () => {
    const ws = makeWorkspace("vat-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      const v = res.body.vat;
      expect(v.slug).toBe("acme-aps");
      expect(v.outputVat).toBe(250);
      expect(v.inputVat).toBe(100);
      expect(v.payable).toBe(150);
      // 2026-03-15 falls in Q1 — quarterly is the only VAT cadence.
      expect(v.periodLabel).toBe("Q1 2026");
      // The full SKAT TastSelv rubrics are surfaced so the owner can file
      // straight from the cockpit — salgsmoms/købsmoms/momstilsvar plus the
      // foreign-trade rubrics A/B/C, the same numbers `vat momsangivelse` gives.
      expect(v.rubrikker.salgsmoms).toBe(250);
      expect(v.rubrikker.kobsmoms).toBe(100);
      expect(v.rubrikker.momstilsvar).toBe(150);
      expect(v.rubrikker.momsAfVarekobUdland).toBe(0);
      expect(v.rubrikker.momsAfYdelseskobUdland).toBe(0);
      expect(v.rubrikker.rubrikA).toBe(0);
      expect(v.rubrikker.rubrikB).toBe(0);
      expect(v.rubrikker.rubrikC).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("vat for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("vat-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/vat",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// #271: a bad-debt write-off books a debit on the output-VAT account. The
// cockpit VAT card must not let that debit drag the headline salgsmoms
// negative — the adjustment belongs on its own clearly-labelled line.
describe("cockpit API — VAT bad-debt adjustment (#271)", () => {
  test("a write-off does not turn salgsmoms negative — it is its own line", async () => {
    const ws = makeWorkspace("vat-baddebt", ["Acme ApS"]);
    try {
      // Q2 2026: genuine sales of 1000 → 250 output VAT, plus a 400 purchase.
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      // Q2 2026: a bad-debt write-off whose VAT relief (250) is large enough
      // that a naive chart-of-accounts sum would net salgsmoms to exactly 0,
      // and a bigger write-off would push it negative. Use 1200 net → 300
      // relief so the booked output-VAT account sum is 250 − 300 = −50.
      postBadDebtWriteoff(ws, "acme-aps", "2026-05-20", 1200);

      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      const v = res.body.vat;
      // The headline salgsmoms is the genuine VAT on sales — never negative.
      expect(v.outputVat).toBe(250);
      // The bad-debt relief sits on its own line, as a negative adjustment.
      expect(v.outputVatAdjustment).toBe(-300);
      // The net payable still reflects the relief: 250 − 300 − 100 = −150.
      expect(v.payable).toBe(-150);
      expect(v.inputVat).toBe(100);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("with no write-off the adjustment line is a clean zero", async () => {
    const ws = makeWorkspace("vat-noadjust", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.vat.outputVat).toBe(250);
      expect(res.body.vat.outputVatAdjustment).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// #272: the cockpit must surface the VAT quarter that is currently due — the
// one with real activity — not a later, near-empty quarter a bad-debt
// write-off happens to touch. It must agree with the static dashboard, which
// keys off the quarter containing the as-of date.
describe("cockpit API — VAT period selection (#272)", () => {
  // The genuine activity is in Q2 2026 — the current quarter (today is in
  // May 2026). A bad-debt write-off lands in the next quarter, Q3; the
  // future-date ceiling is widened so the later-quarter posting is accepted.
  const originalMaxFuture = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
  function withWideFutureWindow<T>(fn: () => T): T {
    process.env.RENTEMESTER_MAX_FUTURE_DAYS = "120";
    try {
      return fn();
    } finally {
      if (originalMaxFuture === undefined) {
        delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;
      } else {
        process.env.RENTEMESTER_MAX_FUTURE_DAYS = originalMaxFuture;
      }
    }
  }

  test("surfaces the active quarter, not a later quarter holding only a write-off", async () => {
    const ws = makeWorkspace("vat-period", ["Acme ApS"]);
    try {
      // The genuine activity is in Q2 2026 (today, May 2026, is in Q2).
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      // A bad-debt write-off lands in Q3 2026 — a later, otherwise-empty
      // quarter. It must NOT pull the surfaced VAT period forward to Q3.
      withWideFutureWindow(() =>
        postBadDebtWriteoff(ws, "acme-aps", "2026-07-15", 800),
      );

      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.status).toBe(200);
      // Q2 2026 (Apr–Jun) is the period that is currently due.
      expect(vatRes.body.vat.periodLabel).toBe("Q2 2026");
      expect(vatRes.body.vat.periodStart).toBe("2026-04-01");
      expect(vatRes.body.vat.periodEnd).toBe("2026-06-30");
      // Q2 → momsangivelse due 1 September 2026.
      expect(vatRes.body.vat.deadline).toBe("2026-09-01");

      // The Overblik VAT card must agree with the dedicated VAT view.
      const ovRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(ovRes.status).toBe(200);
      expect(ovRes.body.overview.vat.periodLabel).toBe("Q2 2026");
      expect(ovRes.body.overview.vat.periodEnd).toBe("2026-06-30");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("the cockpit VAT period agrees with the static dashboard", async () => {
    const ws = makeWorkspace("vat-period-parity", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      withWideFutureWindow(() =>
        postBadDebtWriteoff(ws, "acme-aps", "2026-07-15", 800),
      );

      // The static dashboard's VAT period is keyed off the as-of date.
      const dashRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/dashboard?asOf=2026-05-22",
      );
      expect(dashRes.status).toBe(200);
      // Static dashboard: Q2 (the as-of date's quarter).
      expect(dashRes.body.dashboard.vat.periodStart).toBe("2026-04-01");
      expect(dashRes.body.dashboard.vat.periodEnd).toBe("2026-06-30");

      // The cockpit VAT view must land on the same period.
      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.body.vat.periodStart).toBe(
        dashRes.body.dashboard.vat.periodStart,
      );
      expect(vatRes.body.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #281: the dashboard VAT block must point at the earliest unreported
  // quarter (the one `selectVatQuarter` picks — what the Overblik card and
  // `vat momsangivelse` use), NOT the calendar quarter of the as-of date.
  // When activity lives only in Q1 but the as-of date is in Q2, the old
  // `quarterPeriodForDate` path wrongly surfaced an empty Q2.
  test("dashboard VAT points at the earliest unreported quarter, not the as-of quarter", async () => {
    const ws = makeWorkspace("vat-dash-earliest", ["Acme ApS"]);
    try {
      // The only booked activity is in Q1 2026.
      postPnlEntry(ws, "acme-aps", "2026-02-15", 1000, 400);

      // As-of date is in Q2 — but Q2 has no activity at all.
      const dashRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/dashboard?asOf=2026-05-22",
      );
      expect(dashRes.status).toBe(200);
      // Must surface Q1 2026 — the quarter that is actually due.
      expect(dashRes.body.dashboard.vat.periodStart).toBe("2026-01-01");
      expect(dashRes.body.dashboard.vat.periodEnd).toBe("2026-03-31");

      // And it must agree with the dedicated VAT view + the Overblik card.
      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.body.vat.periodStart).toBe(
        dashRes.body.dashboard.vat.periodStart,
      );
      expect(vatRes.body.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
      const ovRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(ovRes.body.overview.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — documents (GET .../documents)", () => {
  test("returns ingested documents with their link state", async () => {
    const ws = makeWorkspace("doc-live", ["Acme ApS"]);
    try {
      // postPnlEntry ingests one minimal document for the P&L entries.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/documents",
      );
      expect(res.status).toBe(200);
      const d = res.body.documents;
      expect(d.slug).toBe("acme-aps");
      expect(d.documents.length).toBeGreaterThan(0);
      expect(d.documents[0]).toHaveProperty("documentNo");
      expect(d.documents[0]).toHaveProperty("journalEntryNo");
      expect(d).toHaveProperty("linkedCount");
      expect(d).toHaveProperty("unlinkedCount");
      // Each row carries the linked journal entry's text + total fields, so
      // the Bilag view can show what the receipt is for (null when unlinked).
      expect(d.documents[0]).toHaveProperty("journalEntryText");
      expect(d.documents[0]).toHaveProperty("journalEntryTotal");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("documents for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("doc-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/documents",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/**
 * Seeds a read-only archived fiscal year (#197) directly into a company's
 * `import_archive_*` tables — the same shape `archiveDineroYears` writes. The
 * `balances` are `[accountNo, accountName, amount]` SaldoBalance lines; the
 * `postings` are `[accountNo, amount]` archived Posteringer rows.
 */
function seedArchiveYear(
  ws: string,
  slug: string,
  fiscalYear: number,
  balances: Array<[string, string, number]>,
  postings: Array<[string, number]> = [],
) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const info = db
      .query(
        `INSERT INTO import_archive_years
           (source_system, fiscal_year, posting_count, balance_count)
         VALUES ('dinero', ?, ?, ?)`,
      )
      .run(fiscalYear, postings.length, balances.length);
    const yearId = Number(info.lastInsertRowid);
    balances.forEach(([accountNo, name, amount], i) => {
      db.query(
        `INSERT INTO import_archive_balances
           (archive_year_id, line_no, account_no, account_name, amount)
         VALUES (?, ?, ?, ?, ?)`,
      ).run(yearId, i, accountNo, name, amount);
    });
    postings.forEach(([accountNo, amount], i) => {
      db.query(
        `INSERT INTO import_archive_postings
           (archive_year_id, line_no, account_no, amount)
         VALUES (?, ?, ?, ?)`,
      ).run(yearId, i, accountNo, amount);
    });
  } finally {
    db.close();
  }
}

describe("cockpit API — archive (GET .../archive/:year)", () => {
  test("returns the archived year's SaldoBalance and posting summary", async () => {
    const ws = makeWorkspace("arc-live", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [
          ["1000", "Omsætning", -5000],
          ["3000", "Vareforbrug", 1200],
        ],
        [
          ["1000", -5000],
          ["3000", 1200],
        ],
      );
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/2024",
      );
      expect(res.status).toBe(200);
      const a = res.body.archive;
      expect(a.slug).toBe("acme-aps");
      expect(a.year).toBe("2024");
      expect(a.saldoBalance).toHaveLength(2);
      expect(a.saldoBalance[0]).toEqual({
        accountNo: "1000",
        name: "Omsætning",
        amount: -5000,
      });
      expect(a.postings.count).toBe(2);
      expect(a.postings.grossTotal).toBe(6200);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unarchived year is a safe 404", async () => {
    const ws = makeWorkspace("arc-noyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/2099",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a malformed year in the path is a safe 400", async () => {
    const ws = makeWorkspace("arc-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archive for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("arc-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/archive/2024",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — multi-year (GET .../multi-year)", () => {
  test("returns key figures per year, oldest-first, live + archive", async () => {
    const ws = makeWorkspace("my-live", ["Acme ApS"]);
    try {
      // Archived 2025 — income account 1000 closes at −800, expense 3000 at 200.
      seedArchiveYear(ws, "acme-aps", 2025, [
        ["1000", "Omsætning", -800],
        ["3000", "Vareforbrug", 200],
      ]);
      // Live 2026.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const m = res.body.multiYear;
      expect(m.slug).toBe("acme-aps");
      expect(m.years.map((y: any) => y.year)).toEqual(["2025", "2026"]);
      const y2025 = m.years[0];
      expect(y2025.source).toBe("archive");
      expect(y2025.omsaetning).toBe(800);
      expect(y2025.udgifter).toBe(200);
      expect(y2025.resultat).toBe(600);
      const y2026 = m.years[1];
      expect(y2026.source).toBe("live");
      expect(y2026.omsaetning).toBe(1000);
      expect(y2026.udgifter).toBe(400);
      expect(y2026.resultat).toBe(600);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("returns balance-sheet figures and key ratios per year", async () => {
    const ws = makeWorkspace("my-balance", ["Acme ApS"]);
    try {
      // Archived 2025 — income −1000, expense 250, asset 2000 at 700, equity
      // 5000 closing at −150 (credit-signed −150 → +150 egenkapital section).
      // resultat = 1000 − 250 = 750; egenkapital = 150 + 750 = 900;
      // balancesum = 700; egenkapitalandel = 900 / 700.
      seedArchiveYear(ws, "acme-aps", 2025, [
        ["1000", "Omsætning", -1000],
        ["3000", "Vareforbrug", 250],
        ["2000", "Bank", 700],
        ["5000", "Egenkapital", -150],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const y2025 = res.body.multiYear.years[0];
      expect(y2025.balancesum).toBe(700);
      expect(y2025.egenkapital).toBe(900);
      expect(y2025.bruttomargin).toBeCloseTo(0.75, 5);
      expect(y2025.egenkapitalandel).toBeCloseTo(900 / 700, 5);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a ratio with a zero denominator is null, not a fabricated figure", async () => {
    const ws = makeWorkspace("my-ratio-null", ["Acme ApS"]);
    try {
      // No income and no assets — both ratios must be null.
      seedArchiveYear(ws, "acme-aps", 2025, [["3000", "Vareforbrug", 250]]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const y2025 = res.body.multiYear.years[0];
      expect(y2025.bruttomargin).toBeNull();
      expect(y2025.egenkapitalandel).toBeNull();
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty ledger yields no years", async () => {
    const ws = makeWorkspace("my-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      expect(res.body.multiYear.years).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("multi-year for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("my-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/multi-year",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/** Issues one sales invoice into a workspace company's ledger. */
function issueTestInvoice(
  ws: string,
  slug: string,
  issueDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const vat = net * 0.25;
    const result = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate,
      seller: {
        name: "Acme ApS",
        address: "Testvej 1, 2100 København Ø",
        vatOrCvr: "DK12345678",
      },
      buyer: {
        name: "Kunde A/S",
        address: "Købervej 9, 8000 Aarhus C",
        vatOrCvr: "DK87654321",
      },
      lines: [
        {
          description: "Ydelse",
          quantity: 1,
          unitPriceExVat: net,
          lineTotalExVat: net,
        },
      ],
      totals: { netAmount: net, vatRate: 0.25, vatAmount: vat, grossAmount: net + vat },
      currency: "DKK",
    });
    if (!result.ok) throw new Error("issue failed: " + result.errors.join("; "));
  } finally {
    db.close();
  }
}

describe("cockpit API — invoices (GET .../invoices)", () => {
  test("returns issued invoices with their status for the year", async () => {
    const ws = makeWorkspace("inv-live", ["Acme ApS"]);
    try {
      issueTestInvoice(ws, "acme-aps", "2026-03-15", 1000);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/invoices?year=2026",
      );
      expect(res.status).toBe(200);
      const inv = res.body.invoices;
      expect(inv.slug).toBe("acme-aps");
      expect(inv.selectedYear).toBe("2026");
      expect(inv.archived).toBe(false);
      expect(inv.invoices.length).toBe(1);
      expect(inv.invoices[0]).toHaveProperty("invoiceNo");
      expect(inv.invoices[0]).toHaveProperty("status");
      expect(inv.invoices[0].grossAmount).toBe(1250);
      expect(inv.totalGross).toBe(1250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no issued invoices returns an empty list", async () => {
    const ws = makeWorkspace("inv-empty", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/invoices?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.invoices.invoices).toEqual([]);
      expect(res.body.invoices.totalGross).toBe(0);
      expect(res.body.invoices.overdueCount).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("invoices for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("inv-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/invoices",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — contacts (GET .../contacts)", () => {
  test("returns customers and vendors from the master data", async () => {
    const ws = makeWorkspace("con-live", ["Acme ApS"]);
    try {
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        createCustomer(db, { name: "Kunde A/S", vatOrCvr: "DK87654321" });
        createVendor(db, { name: "Leverandør ApS", vatOrCvr: "DK11223344" });
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      const c = res.body.contacts;
      expect(c.slug).toBe("acme-aps");
      expect(c.customers.length).toBe(1);
      expect(c.customers[0].name).toBe("Kunde A/S");
      expect(c.vendors.length).toBe(1);
      expect(c.vendors[0].name).toBe("Leverandør ApS");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no contacts returns empty lists", async () => {
    const ws = makeWorkspace("con-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      expect(res.body.contacts.customers).toEqual([]);
      expect(res.body.contacts.vendors).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("contacts for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("con-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/contacts",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #439 — Kontakter-siden skal kunne svare på "hvad skylder den her kunde mig?"
  // direkte. Server-side aggregerer åbne fakturaer pr. kunde fra den samme
  // ledger-kilde som /invoices, så hvert ContactCustomerRow får openBalance,
  // openInvoiceCount og overdueCount. Kunder med forfaldne fakturaer sorteres
  // øverst (mirrors PortfolioView.sortByAttention).
  test("aggregates open + overdue invoices per customer (#439)", async () => {
    const ws = makeWorkspace("con-saldo", ["Acme ApS"]);
    try {
      // Tre kunder i kontaktlisten — én uden fakturaer, én med åben (ikke-
      // forfalden) faktura, én med forfalden faktura. Navn er join-nøglen
      // mellem invoices og customers (samme regel som "Send på mail"-prefill).
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        createCustomer(db, { name: "Ingen Skyld ApS" });
        createCustomer(db, { name: "Åben Saldo ApS" });
        createCustomer(db, { name: "Forfalden Saldo ApS" });
      } finally {
        db.close();
      }
      // En faktura med en udstedelsesdato langt tilbage er forfalden i dag.
      issueTestInvoiceForBuyer(ws, "acme-aps", "Forfalden Saldo ApS", "2020-01-15", 800);
      // En faktura udstedt i går — antagelig ikke forfalden endnu (30 dages
      // standard betalingsfrist på dansk faktura). buildInvoiceList vurderer
      // selv via getInvoiceStatus.
      const today = new Date();
      const yyyy = today.getUTCFullYear();
      const mm = String(today.getUTCMonth() + 1).padStart(2, "0");
      const dd = String(today.getUTCDate()).padStart(2, "0");
      issueTestInvoiceForBuyer(ws, "acme-aps", "Åben Saldo ApS", `${yyyy}-${mm}-${dd}`, 1200);

      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/contacts",
      );
      expect(res.status).toBe(200);
      const customers = res.body.contacts.customers as Array<{
        name: string;
        openBalance: number;
        openInvoiceCount: number;
        overdueCount: number;
      }>;
      expect(customers.length).toBe(3);

      // Kunden med forfaldne fakturaer ligger øverst.
      expect(customers[0].name).toBe("Forfalden Saldo ApS");
      expect(customers[0].overdueCount).toBe(1);
      expect(customers[0].openInvoiceCount).toBe(1);
      expect(customers[0].openBalance).toBeGreaterThan(0);

      // Derefter kunden med åben (men ikke forfalden) faktura.
      expect(customers[1].name).toBe("Åben Saldo ApS");
      expect(customers[1].overdueCount).toBe(0);
      expect(customers[1].openInvoiceCount).toBe(1);
      expect(customers[1].openBalance).toBeGreaterThan(0);

      // Sidst kunden uden udestående.
      expect(customers[2].name).toBe("Ingen Skyld ApS");
      expect(customers[2].overdueCount).toBe(0);
      expect(customers[2].openInvoiceCount).toBe(0);
      expect(customers[2].openBalance).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

/**
 * Like `issueTestInvoice`, but the buyer name is parametrized so a single
 * workspace can carry invoices for several customers (#439). The seller and
 * VAT shape stay constant — only the buyer + amount + issue date vary.
 */
function issueTestInvoiceForBuyer(
  ws: string,
  slug: string,
  buyerName: string,
  issueDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const vat = net * 0.25;
    const result = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate,
      seller: {
        name: "Acme ApS",
        address: "Testvej 1, 2100 København Ø",
        vatOrCvr: "DK12345678",
      },
      buyer: {
        name: buyerName,
        address: "Købervej 9, 8000 Aarhus C",
      },
      lines: [
        {
          description: "Ydelse",
          quantity: 1,
          unitPriceExVat: net,
          lineTotalExVat: net,
        },
      ],
      totals: { netAmount: net, vatRate: 0.25, vatAmount: vat, grossAmount: net + vat },
      currency: "DKK",
    });
    if (!result.ok) throw new Error("issue failed: " + result.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Posts a balanced liability accrual into a company's ledger: a credit to a
 * liability account (created on the fly if absent) against a debit to bank
 * account `2000`. Neither side is an income/expense account, so no document
 * is required. This is the shape that surfaces in the obligations endpoint.
 */
function postLiability(
  ws: string,
  slug: string,
  transactionDate: string,
  accountNo: string,
  accountName: string,
  amount: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    db.query(
      `INSERT OR IGNORE INTO accounts (account_no, name, type, normal_balance)
       VALUES (?, ?, 'liability', 'credit')`,
    ).run(accountNo, accountName);
    const entry = postJournalEntry(db, {
      transactionDate,
      text: `Hensættelse ${accountName}`,
      lines: [
        { accountNo: "2000", debitAmount: amount },
        { accountNo, creditAmount: amount },
      ],
    });
    if (!entry.ok) throw new Error("liability post failed: " + entry.errors.join("; "));
  } finally {
    db.close();
  }
}

describe("cockpit API — obligations (GET .../obligations)", () => {
  test("surfaces VAT with its statutory deadline and liability payables", async () => {
    const ws = makeWorkspace("obl-live", ["Acme ApS"]);
    try {
      // postPnlEntry → 250 output VAT − 100 input VAT = 150 payable for Q1.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      postLiability(ws, "acme-aps", "2026-06-30", "63060", "Skyldig selskabsskat", 2000);
      postLiability(ws, "acme-aps", "2026-06-30", "63000", "Kreditorer", 500);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/obligations?year=2026",
      );
      expect(res.status).toBe(200);
      const o = res.body.obligations;
      expect(o.slug).toBe("acme-aps");
      expect(o.archived).toBe(false);
      const vat = o.obligations.find((r: any) => r.kind === "vat");
      expect(vat.amount).toBe(150);
      // Q1 2026 (Jan–Mar) is filed/paid by 1 June 2026.
      expect(vat.dueDate).toBe("2026-06-01");
      const tax = o.obligations.find((r: any) => r.kind === "corporation-tax");
      expect(tax.amount).toBe(2000);
      expect(tax.dueDate).toBe("2027-11-01");
      const creditors = o.obligations.find((r: any) => r.kind === "creditors");
      expect(creditors.amount).toBe(500);
      expect(creditors.dueDate).toBeNull();
      expect(o.totalOwed).toBe(2650);
      // Sorted soonest-first: dated rows before the dateless creditor row.
      expect(o.obligations[o.obligations.length - 1].kind).toBe("creditors");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("VAT is not double-counted: gross 64xxx VAT accounts never become liability rows", async () => {
    // A Dinero-imported chart books VAT into the standard Danish 64xxx block,
    // where the VAT accounts are typed `liability` (not `vat`). The net VAT
    // obligation is already surfaced by `vatPositionForPeriod`; the gross
    // output/input/reverse-charge accounts are merely its *components* and
    // must NOT also appear as their own per-account obligations — counting
    // both double-counts VAT (the Helheim 2026 "Skyldige beløb i alt" bug).
    const ws = makeWorkspace("obl-vat-dedupe", ["Acme ApS"]);
    try {
      // Gross output-side 64xxx VAT accounts, liability-typed, with credit
      // balances — the exact shape of the Helheim 2026 bug. They feed the
      // *net* VAT computation (here output-only: 4457.25 + 62.50 = 4519.75
      // payable for H1) and must NOT also surface as their own per-account
      // obligations.
      postLiability(ws, "acme-aps", "2026-06-30", "64000", "Salgsmoms (udgående moms)", 4457.25);
      postLiability(ws, "acme-aps", "2026-06-30", "64040", "Moms af ydelser fra udlandet", 62.5);
      // A genuine, non-VAT liability that MUST still surface unchanged.
      postLiability(ws, "acme-aps", "2026-06-30", "63060", "Skyldig selskabsskat", 264);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/obligations?year=2026",
      );
      expect(res.status).toBe(200);
      const o = res.body.obligations;
      // Exactly one VAT row — the dedicated net obligation.
      const vatRows = o.obligations.filter((r: any) => r.kind === "vat");
      expect(vatRows.length).toBe(1);
      expect(vatRows[0].amount).toBe(4519.75);
      // No gross 64xxx account leaks through as its own liability row.
      expect(
        o.obligations.some(
          (r: any) =>
            r.accountNo !== null &&
            r.accountNo >= "64000" &&
            r.accountNo < "64100",
        ),
      ).toBe(false);
      // The genuine non-VAT liability still surfaces.
      const tax = o.obligations.find((r: any) => r.kind === "corporation-tax");
      expect(tax.amount).toBe(264);
      // Total = net VAT (4519.75) + corporation tax (264), VAT counted ONCE.
      // Pre-fix this was 4519.75 + 4457.25 + 62.50 (gross leaking) + 264.
      expect(o.totalOwed).toBe(4783.75);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company that owes nothing still surfaces the annual-report deadline", async () => {
    const ws = makeWorkspace("obl-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/obligations?year=2026",
      );
      expect(res.status).toBe(200);
      // Nothing is owed (no VAT, no liabilities) — totalOwed is 0 — but the
      // årsrapport filing deadline (#290) is a recurring legal duty with no
      // ledger amount, so it is always shown.
      expect(res.body.obligations.totalOwed).toBe(0);
      const rows = res.body.obligations.obligations;
      expect(rows.length).toBe(1);
      expect(rows[0].kind).toBe("annual-report");
      expect(rows[0].amount).toBe(0);
      expect(rows[0].dueDate).toBe("2027-05-01");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("obligations for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("obl-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/obligations",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — cash flow (GET .../cashflow)", () => {
  test("computes monthly in/out and the balance trajectory from bank rows", async () => {
    const ws = makeWorkspace("cf-live", ["Acme ApS"]);
    try {
      seedBankTransaction(ws, "acme-aps", "2026-02-10", "Indbetaling", 1000, 1000);
      seedBankTransaction(ws, "acme-aps", "2026-02-20", "Gebyr", -200, 800);
      seedBankTransaction(ws, "acme-aps", "2026-05-05", "Indbetaling", 500, 1300);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.slug).toBe("acme-aps");
      expect(cf.archived).toBe(false);
      expect(cf.hasTransactions).toBe(true);
      expect(cf.months.length).toBe(12);
      // February: 1000 in, 200 out, 800 net.
      const feb = cf.months[1];
      expect(feb.indbetalinger).toBe(1000);
      expect(feb.udbetalinger).toBe(200);
      expect(feb.netto).toBe(800);
      // May: 500 in only.
      expect(cf.months[4].indbetalinger).toBe(500);
      // Year totals + closing balance from the latest balance_after.
      expect(cf.totalIn).toBe(1500);
      expect(cf.totalOut).toBe(200);
      expect(cf.closingBalance).toBe(1300);
      expect(cf.balanceSeries.length).toBe(3);
      expect(cf.balanceSeries[cf.balanceSeries.length - 1].balance).toBe(1300);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("opening balance is the actual balance before the year starts", async () => {
    const ws = makeWorkspace("cf-opening", ["Acme ApS"]);
    try {
      seedBankTransaction(ws, "acme-aps", "2025-12-15", "Primo", 400, 400);
      seedBankTransaction(ws, "acme-aps", "2026-03-01", "Indbetaling", 600, 1000);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.openingBalance).toBe(400);
      expect(cf.closingBalance).toBe(1000);
      expect(cf.totalIn).toBe(600);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no bank transactions reports an empty cash flow", async () => {
    const ws = makeWorkspace("cf-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.hasTransactions).toBe(false);
      expect(cf.totalIn).toBe(0);
      expect(cf.totalOut).toBe(0);
      expect(cf.balanceSeries).toEqual([]);
      expect(cf.months.length).toBe(12);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("cashflow for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("cf-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/cashflow",
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — VAT deadline (GET .../vat & .../overview)", () => {
  test("vat carries the statutory filing deadline and a countdown", async () => {
    const ws = makeWorkspace("vat-deadline", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      // Q1 2026 (Jan–Mar) → filed/paid by 1 June 2026.
      expect(res.body.vat.deadline).toBe("2026-06-01");
      expect(typeof res.body.vat.daysRemaining).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview's VAT card and receivables block carry the new fields", async () => {
    const ws = makeWorkspace("ov-deadline", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      // Q1 2026 (Jan–Mar) → filed/paid by 1 June 2026.
      expect(res.body.overview.vat.deadline).toBe("2026-06-01");
      // No issued invoices → a clean zero receivables block.
      expect(res.body.overview.receivables).toEqual({
        openCount: 0,
        openTotal: 0,
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("cockpit API — company onboarding (POST /api/companies)", () => {
  test("creates a new company in the workspace", async () => {
    const ws = makeWorkspace("add-create");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "Gamma ApS", cvr: "DK12345678" }),
      });
      expect(res.status).toBe(201);
      expect(res.body.company.slug).toBe("gamma-aps");

      const list = await get(config({ workspaceRoot: ws }), "/api/companies");
      expect(list.body.companies.map((c: any) => c.slug)).toContain("gamma-aps");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing name is a safe 400", async () => {
    const ws = makeWorkspace("add-noname");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({}),
      });
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a duplicate slug is a conflict with no path leak", async () => {
    const ws = makeWorkspace("add-dup", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "Acme ApS" }),
      });
      expect(res.status).toBe(409);
      expect(res.body.error.code).toBe("conflict");
      expect(JSON.stringify(res.body)).not.toContain(ws);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a malformed JSON body is a safe 400", async () => {
    const ws = makeWorkspace("add-badjson");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{not json",
      });
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("POST onboarding is gated by the auth seam too", async () => {
    const ws = makeWorkspace("add-auth");
    try {
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await get(cfg, "/api/companies", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "Delta ApS" }),
      });
      expect(res.status).toBe(401);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
