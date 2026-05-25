// Tests: src/server/router.ts (statement-export route dispatch),
// src/server/data/statement-exports.ts (CSV-byggerne for #372).
//
// Krav fra issue #372 (CSV-slice; PDF-slice udskudt):
// - Resultatopgørelse, Balance og Saldobalance har hver et nyt GET-endpoint
//   `…/export?format=csv` der returnerer en CSV-fil med stabil dansk header,
//   content-type text/csv og en content-disposition attachment.
// - CSV starter med en UTF-8 BOM så Excel/Numbers åbner den uden mojibake.
// - Output er deterministisk: samme ledger + samme år + samme udtrækningsdato
//   ⇒ byte-identisk CSV.
// - PDF afvises pænt med en 400 og en venlig dansk besked indtil PDF-slice'n
//   lander i et opfølger-issue.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { type ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import {
  exportBalanceCsv,
  exportIncomeStatementCsv,
  exportJournalCsv,
  exportTrialBalanceCsv,
} from "../../src/server/data/statement-exports";

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    workspaceRoot,
    authRequired: false,
    authToken: null,
  };
}

/**
 * Post tiny P&L entries (salg + køb) to a company's ledger so the three
 * reports are non-empty. Mirrors the real `postPnlEntry`-helper used by
 * `server-api.test.ts`: a synthetic source document is ingested first
 * because the ledger requires `document_id` on income/expense lines.
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
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-stmt-csv-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    writeFileSync(sourceFile, `Bilag ${transactionDate}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: transactionDate,
      invoiceNo: `STMT-${transactionDate}`,
      deliveryDescription: "Statement-CSV testbilag",
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
        text: "Salg",
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
        text: "Køb",
        documentId: doc.documentId,
        lines: [
          { accountNo: "3000", debitAmount: expense, vatCode: "DK_PURCHASE_25" },
          { accountNo: "4000", debitAmount: expense * 0.25 },
          { accountNo: "2000", creditAmount: expense * 1.25 },
        ],
      });
      if (!purchase.ok)
        throw new Error("purchase post failed: " + purchase.errors.join("; "));
    }
  } finally {
    db.close();
  }
}

async function fetchRaw(cfg: ServerConfig, path: string): Promise<Response> {
  return handleRequest(new Request(`http://localhost${path}`), cfg);
}

describe("#372 — Resultatopgørelse CSV-eksport (GET …/income-statement/export)", () => {
  test("returnerer en CSV-attachment med dansk header-række", async () => {
    const ws = makeWorkspace("ie-csv", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/income-statement/export?format=csv&year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/csv");
      const cd = res.headers.get("content-disposition") ?? "";
      expect(cd).toContain("attachment");
      expect(cd).toContain("resultatopgorelse-acme-aps-2026.csv");
      const body = await res.text();
      // UTF-8 BOM
      expect(body.charCodeAt(0)).toBe(0xfeff);
      // Excel auto-detect: separator-hint helt øverst.
      expect(body).toContain("sep=;");
      // Stabile danske header-felter.
      expect(body).toContain("Konto;Navn;Beløb 2026;Beløb 2025");
      expect(body).toContain("Indtægter");
      expect(body).toContain("Udgifter");
      expect(body).toContain("Årets resultat");
      // Beløbene fra ledgeren — formatteret med dansk decimalkomma.
      expect(body).toContain("1000,00");
      expect(body).toContain("400,00");
      // Metadata-blok øverst.
      expect(body).toContain("Virksomhed;Acme ApS");
      expect(body).toContain("Rapport;Resultatopgørelse");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("format=csv er default når parameteret er udeladt", async () => {
    const ws = makeWorkspace("ie-default", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 500, 0);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/income-statement/export?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/csv");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("format=pdf returnerer en application/pdf-attachment (#463)", async () => {
    const ws = makeWorkspace("ie-pdf", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/income-statement/export?format=pdf&year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toBe("application/pdf");
      const cd = res.headers.get("content-disposition") ?? "";
      expect(cd).toContain("attachment");
      expect(cd).toContain(".pdf");
      const bytes = await res.arrayBuffer();
      const head = Buffer.from(bytes).slice(0, 5).toString("ascii");
      // %PDF- header skal være først.
      expect(head).toBe("%PDF-");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ukendt slug giver en safe 404", async () => {
    const ws = makeWorkspace("ie-404", []);
    try {
      const res = await fetchRaw(
        config(ws),
        "/api/companies/ghost/income-statement/export?format=csv",
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#372 — Balance CSV-eksport (GET …/balance/export)", () => {
  test("returnerer en CSV med sektioner og ultimo-datoen i header", async () => {
    const ws = makeWorkspace("bal-csv", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/balance/export?format=csv&year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/csv");
      expect(res.headers.get("content-disposition")).toContain(
        "balance-acme-aps-2026.csv",
      );
      const body = await res.text();
      expect(body).toContain("Aktiver");
      expect(body).toContain("Passiver");
      expect(body).toContain("Egenkapital");
      expect(body).toContain("Passiver og egenkapital i alt");
      expect(body).toContain("Rapport;Balance");
      // Ultimo-kolonnen bruger den faktiske ultimo-dato fra builderen.
      expect(body).toContain("Pr. 2026-12-31");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#372 — Saldobalance CSV-eksport (GET …/trial-balance/export)", () => {
  test("returnerer en CSV med debet/kredit/saldo og total-linje", async () => {
    const ws = makeWorkspace("tb-csv", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/trial-balance/export?format=csv&year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/csv");
      expect(res.headers.get("content-disposition")).toContain(
        "saldobalance-acme-aps-2026.csv",
      );
      const body = await res.text();
      expect(body).toContain("Konto;Navn;Type;Debet;Kredit;Saldo");
      expect(body).toContain("Rapport;Saldobalance");
      expect(body).toContain("I alt");
      // Bank-kontoen (2000) modtog både salgs- og købs-bevægelser.
      expect(body).toContain("2000");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#465 — Posteringer CSV-eksport (GET …/journal/export)", () => {
  test("returnerer en CSV med dansk header-række og en linje pr. konto-bevægelse", async () => {
    const ws = makeWorkspace("jrn-csv", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/journal/export?format=csv&year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toContain("text/csv");
      const cd = res.headers.get("content-disposition") ?? "";
      expect(cd).toContain("attachment");
      expect(cd).toContain("posteringer-acme-aps-2026.csv");
      const body = await res.text();
      // BOM + sep=; preamble for Excel.
      expect(body.charCodeAt(0)).toBe(0xfeff);
      expect(body).toContain("sep=;");
      expect(body).toContain("Rapport;Posteringer");
      expect(body).toContain("Dato;Bilag;Konto;Kontonavn;Tekst;Debet;Kredit");
      // Salgs- og købsposterne berører de seedede konti.
      expect(body).toContain("2000");
      expect(body).toContain("1000");
      expect(body).toContain("3000");
      // Total-linjen lukker filen.
      expect(body).toContain("I alt");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("account=<kontonr> filtrerer til posteringer der rør den konto", async () => {
    const ws = makeWorkspace("jrn-csv-acc", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 0); // kun salg
      postPnlEntry(ws, "acme-aps", "2026-03-16", 0, 400); // kun køb
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/journal/export?format=csv&year=2026&account=3000",
      );
      expect(res.status).toBe(200);
      const body = await res.text();
      expect(body).toContain("Kontofilter;3000");
      // Salget berørte ikke 3000 (det er en udgiftskonto) — det skal være væk.
      expect(body).toContain("3000");
      expect(body).not.toContain("Salg");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("format=pdf afvises på journal — kun csv understøttes", async () => {
    const ws = makeWorkspace("jrn-pdf-reject", ["Acme ApS"]);
    try {
      const res = await fetchRaw(
        config(ws),
        "/api/companies/acme-aps/journal/export?format=pdf",
      );
      expect(res.status).toBe(400);
      const body = (await res.json()) as { errors: string[]; code: string };
      expect(body.errors[0]).toContain("pdf");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("eksporteren er deterministisk", () => {
    const ws = makeWorkspace("jrn-det", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const opts = { generatedAtIsoDate: "2026-05-24" };
      const a = exportJournalCsv(ws, "acme-aps", 2026, null, opts);
      const b = exportJournalCsv(ws, "acme-aps", 2026, null, opts);
      expect(a.content).toBe(b.content);
      expect(a.filename).toBe(b.filename);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#372 — CSV-builderne er deterministiske", () => {
  test("samme ledger + samme udtrækningsdato giver byte-identisk CSV", () => {
    const ws = makeWorkspace("det", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const opts = { generatedAtIsoDate: "2026-05-24" };
      const a = exportIncomeStatementCsv(ws, "acme-aps", 2026, opts);
      const b = exportIncomeStatementCsv(ws, "acme-aps", 2026, opts);
      expect(a.content).toBe(b.content);
      expect(a.filename).toBe(b.filename);

      const balA = exportBalanceCsv(ws, "acme-aps", 2026, opts);
      const balB = exportBalanceCsv(ws, "acme-aps", 2026, opts);
      expect(balA.content).toBe(balB.content);

      const tbA = exportTrialBalanceCsv(ws, "acme-aps", 2026, opts);
      const tbB = exportTrialBalanceCsv(ws, "acme-aps", 2026, opts);
      expect(tbA.content).toBe(tbB.content);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rute-kataloget annoncerer de tre nye /export-endpoints", async () => {
    const ws = makeWorkspace("catalog");
    try {
      const res = await fetchRaw(config(ws), "/api/health");
      const body = (await res.json()) as {
        routes: Array<{ method: string; pattern: string }>;
      };
      const patterns = body.routes.map((r) => r.pattern);
      expect(patterns).toContain(
        "/api/companies/:slug/income-statement/export",
      );
      expect(patterns).toContain("/api/companies/:slug/balance/export");
      expect(patterns).toContain("/api/companies/:slug/trial-balance/export");
      expect(patterns).toContain("/api/companies/:slug/journal/export");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
