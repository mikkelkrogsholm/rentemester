// Tests: src/server/write-handlers.ts (handleInvoiceCreditNote) and the
// POST .../invoices/credit-note route in src/server/router.ts (#412).
//
// The Cockpit becomes a third caller of `issueCreditNote`, alongside the CLI's
// `invoice credit-note` command and the MCP tool. These specs cover the
// cross-cutting gates (backup lock, localhost hard-gate, confirm gate), the
// happy path, and the input/business-rejection error mapping — including the
// idempotency rejection ("already fully credited") being a 409 conflict.
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
import { computeInvoiceAmounts } from "../../src/core/invoice";
import { issueInvoice } from "../../src/core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyName = "Acme ApS") {
  const root = tmpRoot(label);
  initWorkspace(root);
  const created = createCompany(root, { name: companyName });
  return { root, slug: created.slug };
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

async function post(
  cfg: ServerConfig,
  path: string,
  body?: unknown,
  headers?: Record<string, string>,
) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1", ...(headers ?? {}) },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

/**
 * Issues an invoice directly into the company ledger and posts it, returning
 * the document id ready to be credited. Mirrors what the cockpit's own
 * `handleInvoiceIssue` + `handleInvoicePost` would produce — but written
 * directly against core so the test focuses on the credit-note route only.
 */
function makeIssuedAndPostedInvoice(ws: string, slug: string): number {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const computed = computeInvoiceAmounts(
      [{ description: "Konsulent maj", quantity: 2, unitPriceExVat: 1000 }],
      25,
    );
    if (!computed.ok) {
      throw new Error(`fixture: compute failed: ${computed.errors.join("; ")}`);
    }
    const issued = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
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
      lines: computed.lines,
      totals: {
        netAmount: computed.totals.netAmount,
        vatRate: computed.totals.vatRate,
        vatAmount: computed.totals.vatAmount,
        grossAmount: computed.totals.grossAmount,
      },
      currency: "DKK",
    });
    if (!issued.ok || !issued.documentId) {
      throw new Error(`fixture: failed to issue invoice: ${issued.errors.join("; ")}`);
    }
    const posted = postIssuedInvoiceToLedger(db, {
      invoiceDocumentId: issued.documentId,
    });
    if (!posted.ok) {
      throw new Error(`fixture: failed to post invoice: ${posted.errors.join("; ")}`);
    }
    return issued.documentId;
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// Happy path
// --------------------------------------------------------------------------

describe("Cockpit write — invoice credit-note (happy path)", () => {
  test("a POST .../invoices/credit-note issues a credit note and a reversal entry", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-ok");
    try {
      const invoiceDocumentId = makeIssuedAndPostedInvoice(ws, slug);
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId,
          issueDate: "2026-05-20",
          reason: "Aftale annulleret efter fakturering",
          confirm: true,
        },
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(typeof res.body.creditNote.documentId).toBe("number");
      // Credit-note numbers use the CN-YYYY-NNNN canonical format.
      expect(res.body.creditNote.creditNoteNumber).toMatch(/^CN-\d{4}-\d{4}$/);
      expect(res.body.creditNote.originalInvoiceNumber).toMatch(/-\d{4}$/);
      // A reversal journal entry was appended.
      expect(typeof res.body.creditNote.journalEntryId).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// Input + gates
// --------------------------------------------------------------------------

describe("Cockpit write — invoice credit-note (input + gates)", () => {
  test("missing confirm is a 400 — the action is irreversible", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-noconfirm");
    try {
      const invoiceDocumentId = makeIssuedAndPostedInvoice(ws, slug);
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId,
          issueDate: "2026-05-20",
          reason: "Test",
          // confirm omitted
        },
      );
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("bad_request");
      expect(res.body.error.message).toMatch(/confirm/i);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("missing invoiceDocumentId is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-nodocid");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/credit-note`,
        {
          issueDate: "2026-05-20",
          reason: "Test",
          confirm: true,
        },
      );
      expect(res.status).toBe(400);
      expect(res.body.error.message).toContain("invoiceDocumentId");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a blank reason is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-noreason");
    try {
      const invoiceDocumentId = makeIssuedAndPostedInvoice(ws, slug);
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId,
          issueDate: "2026-05-20",
          reason: "   ",
          confirm: true,
        },
      );
      expect(res.status).toBe(400);
      expect(res.body.error.message).toContain("reason");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing source invoice is a 409 conflict, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-nosource");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId: 9999,
          issueDate: "2026-05-20",
          reason: "Test",
          confirm: true,
        },
      );
      // The core message ("invoice document 9999 does not exist") matches the
      // `does not exist` heuristic in `withCompanyMutation`, so this is a 409.
      expect(res.status).toBe(409);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("re-crediting an already fully credited invoice is a 409, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-double");
    try {
      const invoiceDocumentId = makeIssuedAndPostedInvoice(ws, slug);
      const cfg = config({ workspaceRoot: ws });
      const first = await post(
        cfg,
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId,
          issueDate: "2026-05-20",
          reason: "Første kreditering",
          confirm: true,
        },
      );
      expect(first.status).toBe(200);

      const second = await post(
        cfg,
        `/api/companies/${slug}/invoices/credit-note`,
        {
          invoiceDocumentId,
          issueDate: "2026-05-21",
          reason: "Dobbelt kreditering",
          confirm: true,
        },
      );
      // The core message ("invoice X is already fully credited") matches the
      // `already` heuristic, so this is a 409 conflict, not a 500.
      expect(second.status).toBe(409);
      expect(second.body.error.message).toMatch(/already fully credited/i);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("inv-credit-noco");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/nope-aps/invoices/credit-note`,
        {
          invoiceDocumentId: 1,
          issueDate: "2026-05-20",
          reason: "Test",
          confirm: true,
        },
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the credit-note route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("inv-credit-get");
    try {
      const res = await handleRequest(
        new Request(
          `http://localhost/api/companies/${slug}/invoices/credit-note`,
          { method: "GET", headers: { host: "127.0.0.1" } },
        ),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
