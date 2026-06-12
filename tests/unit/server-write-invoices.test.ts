// Tests: src/server/write-handlers.ts (handleInvoiceIssue, handleInvoicePost,
// handleInvoiceSettle) and the POST .../invoices/{issue,post,settle} routes in
// src/server/router.ts (#213, slice 4).
//
// Slice 4 brings invoicing into the human-operated Cockpit. Each of the three
// actions — issue, post, settle — is routed through the shared
// `withCompanyMutation` pipeline and reuses the SAME core functions the CLI
// (`src/cli/invoice.ts`) and MCP use. These specs cover the cross-cutting
// gates (backup lock, localhost hard-gate, confirm gate), the happy path, and
// the input/business-rejection error mapping — including the heuristic that
// classifies an invoice double-post / double-settle as a 409 conflict.
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
import { configureBackupLock } from "../../src/core/backup-governance";
import { createSystemBackup } from "../../src/core/system-backups";

const DAY = 24 * 60 * 60 * 1000;

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

/** A workspace with one company; returns its root + slug. */
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

/** Opens a company ledger, runs `fn`, always closes the handle. */
function withLedger<T>(
  ws: string,
  slug: string,
  fn: (db: ReturnType<typeof openDb>, companyRoot: string) => T,
): T {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    return fn(db, companyRoot);
  } finally {
    db.close();
  }
}

/** Inserts an incoming bank receipt and returns its row id. */
function insertBankReceipt(
  ws: string,
  slug: string,
  date: string,
  ref: string,
  amount: number,
): number {
  return withLedger(ws, slug, (db) => {
    const row = db
      .query(
        "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?) RETURNING id",
      )
      .get(
        date,
        date,
        "Customer payment",
        amount,
        ref,
        `batch-${ref}`,
        `hash-${ref}`,
        `tx-${ref}`,
      ) as { id: number };
    return row.id;
  });
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

/** A minimal valid issue body — one line, default 25% VAT, direct buyer. */
function issueBody(over: Record<string, unknown> = {}) {
  return {
    issueDate: "2026-05-16",
    lines: [{ description: "Bogføring maj", quantity: 2, unitPriceExVat: 1000 }],
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
    ...over,
  };
}

// --------------------------------------------------------------------------
// Issue
// --------------------------------------------------------------------------

describe("Cockpit write — invoice issue (happy path)", () => {
  test("a POST .../invoices/issue computes the totals and issues the invoice", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-ok");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody(),
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      // Rentemester computed every amount from the bare line input — the
      // human never typed net/VAT/gross. 2 * 1000 = 2000 net, 25% = 500 VAT.
      expect(res.body.invoice.netAmount).toBe(2000);
      expect(res.body.invoice.vatAmount).toBe(500);
      expect(res.body.invoice.grossAmount).toBe(2500);
      expect(res.body.invoice.vatRate).toBe(0.25);
      expect(typeof res.body.invoice.documentId).toBe("number");
      // #251: invoice numbers use one canonical four-digit fortløbende format.
      expect(res.body.invoice.invoiceNumber).toMatch(/-\d{4}$/);

      // The issued invoice is in the ledger as an issued_invoice document.
      withLedger(ws, slug, (db) => {
        const row = db
          .query(
            "SELECT COUNT(*) AS n FROM documents WHERE document_type = 'issued_invoice'",
          )
          .get() as { n: number };
        expect(row.n).toBe(1);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("issuing does not require confirm — it is a kladde, not a posting", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-noconfirm");
    try {
      // No `confirm` field at all — issuing is non-destructive at the ledger
      // level, so the confirm gate must NOT apply.
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody(),
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — invoice issue (input + gates)", () => {
  test("a missing issueDate is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-nodate");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody({ issueDate: undefined }),
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      expect(res.body.errors[0]).toContain("issueDate");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty lines array is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-nolines");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody({ lines: [] }),
      );
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("lines");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-numeric quantity is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-badqty");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody({
          lines: [{ description: "X", quantity: "two", unitPriceExVat: 100 }],
        }),
      );
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("quantity");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a core validation rejection (missing buyer) is mapped to a 400, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-nobuyer");
    try {
      // A full invoice requires buyer.name/address — omitting the buyer makes
      // `issueInvoice`'s validation reject; that is a 400, never a 500.
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody({ buyer: undefined }),
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      expect(res.body.errors[0]).toContain("buyer");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown customerId is mapped to a conflict, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-badcust");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody({ customerId: 9999 }),
      );
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("customer");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("inv-issue-noco");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/nope-aps/invoices/issue`,
        issueBody(),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the issue route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/invoices/issue`, {
          method: "GET",
        }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an issue from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-remote");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody(),
        { host: "cockpit.example.com" },
      );
      expect(res.status).toBe(401);
      expect(res.body.code).toBe("unauthorized");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an issue is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("inv-issue-locked");
    try {
      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, {
          createdAt: new Date(Date.now() - 10 * DAY).toISOString(),
        });
      });
      insertBankReceipt(
        ws,
        slug,
        new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10),
        "late",
        500,
      );
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/issue`,
        issueBody(),
      );
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("Bogføring er låst");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// Post
// --------------------------------------------------------------------------

/** Issues an invoice via the route and returns its document id. */
async function issueInvoiceVia(cfg: ServerConfig, slug: string): Promise<number> {
  const res = await post(cfg, `/api/companies/${slug}/invoices/issue`, issueBody());
  expect(res.status).toBe(200);
  return res.body.invoice.documentId as number;
}

describe("Cockpit write — invoice post", () => {
  test("a POST .../invoices/post books the invoice to the ledger", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-ok");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);

      const res = await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(typeof res.body.posting.entryId).toBe("number");

      // A journal entry now links to the invoice document.
      withLedger(ws, slug, (db) => {
        const row = db
          .query(
            "SELECT COUNT(*) AS n FROM journal_entries WHERE document_id = ?",
          )
          .get(documentId) as { n: number };
        expect(row.n).toBe(1);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("without confirm:true the post is refused with 400", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-noconfirm");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);

      const res = await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      // Nothing was posted.
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT COUNT(*) AS n FROM journal_entries")
          .get() as { n: number };
        expect(row.n).toBe(0);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing invoiceDocumentId is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-noid");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/post`,
        { confirm: true },
      );
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("invoiceDocumentId");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("posting a non-existent invoice is mapped to a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-missing");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/invoices/post`,
        { invoiceDocumentId: 4242, confirm: true },
      );
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("does not exist");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("posting the same invoice twice is mapped to a 409 conflict (heuristic: 'already')", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-twice");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);

      const first = await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      expect(first.status).toBe(200);

      // The second post hits the core's `already has journal entry` rejection.
      // The withCompanyMutation heuristic must classify the English "already"
      // message as a 409 conflict, not a 400 — it is a state conflict.
      const second = await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      expect(second.status).toBe(409);
      expect(second.body.code).toBe("conflict");
      expect(second.body.errors[0]).toContain("already");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a post is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("inv-post-locked");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);

      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, {
          createdAt: new Date(Date.now() - 10 * DAY).toISOString(),
        });
      });
      insertBankReceipt(
        ws,
        slug,
        new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10),
        "late",
        500,
      );
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("Bogføring er låst");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// Settle
// --------------------------------------------------------------------------

describe("Cockpit write — invoice settle", () => {
  test("a POST .../invoices/settle settles the invoice against a bank receipt", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-ok");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);
      await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      // The issue body grosses to 2500 — a matching incoming receipt.
      const txId = insertBankReceipt(ws, slug, "2026-05-20", "INV-PAY-1", 2500);

      const res = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: txId,
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.settlement.principalAmount).toBe(2500);
      expect(res.body.settlement.openBalance).toBe(0);
      expect(typeof res.body.settlement.paymentId).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("without confirm:true the settle is refused with 400", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-noconfirm");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);
      await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      const txId = insertBankReceipt(ws, slug, "2026-05-20", "INV-PAY-2", 2500);

      const res = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: txId,
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a settle without a bank transaction reference is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-notx");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);

      const res = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("bankTransaction");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("settling against a non-existent bank transaction is mapped to a 409", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-badtx");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);
      await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });

      const res = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: 9999,
        confirm: true,
      });
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("does not exist");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("settling the same bank receipt twice is mapped to a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-twice");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);
      await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      const txId = insertBankReceipt(ws, slug, "2026-05-20", "INV-PAY-3", 2500);

      const first = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: txId,
        confirm: true,
      });
      expect(first.status).toBe(200);

      // The second settle hits `bank transaction N is already linked …` — an
      // English "already" message the heuristic must map to a 409 conflict.
      const second = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: txId,
        confirm: true,
      });
      expect(second.status).toBe(409);
      expect(second.body.code).toBe("conflict");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a settle is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-locked");
    try {
      const cfg = config({ workspaceRoot: ws });
      const documentId = await issueInvoiceVia(cfg, slug);
      await post(cfg, `/api/companies/${slug}/invoices/post`, {
        invoiceDocumentId: documentId,
        confirm: true,
      });
      const txId = insertBankReceipt(ws, slug, "2026-05-20", "INV-PAY-4", 2500);

      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, {
          createdAt: new Date(Date.now() - 10 * DAY).toISOString(),
        });
      });
      insertBankReceipt(
        ws,
        slug,
        new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10),
        "late",
        500,
      );
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(cfg, `/api/companies/${slug}/invoices/settle`, {
        invoiceDocumentId: documentId,
        bankTransactionId: txId,
        confirm: true,
      });
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("Bogføring er låst");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the settle route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("inv-settle-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/invoices/settle`, {
          method: "GET",
        }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
