// Tests: GET /api/companies/:slug/invoices/:id/pdf — the cockpit's read route
// that serves the issued-invoice PDF so an owner can download it without
// opening the CLI (#378). The route wraps `renderIssuedInvoicePdf`, so the
// bytes it serves match what `bun run cli invoice render` would produce.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";

function makeWorkspace(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS" });
  return { root, slug: created.slug };
}

function config(workspaceRoot: string): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    workspaceRoot,
  };
}

/** Issues a single invoice into the company; returns its document id. */
function issueSample(ws: string, slug: string): number {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const result = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0001",
      seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9" },
      lines: [
        {
          description: "Bogføring",
          quantity: 1,
          unitPriceExVat: 1000,
          lineTotalExVat: 1000,
        },
      ],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });
    if (!result.ok || !result.documentId) {
      throw new Error(`issue failed: ${(result.errors ?? []).join("; ")}`);
    }
    return result.documentId;
  } finally {
    db.close();
  }
}

async function getRaw(cfg: ServerConfig, path: string): Promise<Response> {
  return handleRequest(
    new Request(`http://localhost${path}`, { headers: { host: "127.0.0.1" } }),
    cfg,
  );
}

describe("cockpit API — issued invoice PDF (GET .../invoices/:id/pdf)", () => {
  test("serves the issued-invoice PDF inline as application/pdf", async () => {
    const { root: ws, slug } = makeWorkspace("invpdf-ok");
    try {
      const id = issueSample(ws, slug);
      const res = await getRaw(
        config(ws),
        `/api/companies/${slug}/invoices/${id}/pdf`,
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toBe("application/pdf");
      expect(res.headers.get("content-disposition")).toContain("inline");
      expect(res.headers.get("content-disposition")).toContain("2026-0001.pdf");
      expect(res.headers.get("x-content-type-options")).toBe("nosniff");
      const body = new Uint8Array(await res.arrayBuffer());
      const head = new TextDecoder("latin1").decode(body.subarray(0, 5));
      expect(head).toBe("%PDF-");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown invoice id is a safe 404", async () => {
    const { root: ws, slug } = makeWorkspace("invpdf-404");
    try {
      const res = await getRaw(
        config(ws),
        `/api/companies/${slug}/invoices/9999/pdf`,
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company is a safe 404", async () => {
    const { root: ws } = makeWorkspace("invpdf-co404");
    try {
      const res = await getRaw(
        config(ws),
        "/api/companies/ghost/invoices/1/pdf",
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-GET method is rejected", async () => {
    const { root: ws, slug } = makeWorkspace("invpdf-method");
    try {
      const res = await handleRequest(
        new Request(
          `http://localhost/api/companies/${slug}/invoices/1/pdf`,
          { method: "POST", headers: { host: "127.0.0.1" } },
        ),
        config(ws),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
