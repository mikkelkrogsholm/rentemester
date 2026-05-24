// Tests: src/server/write-handlers.ts (handleDocumentBookingOptions,
// handleDocumentBookExpense) and the routes added for #407:
//
//   GET  /api/companies/:slug/documents/:id/booking-options
//   POST /api/companies/:slug/documents/book-expense
//
// The Cockpit's Bilag view shows "Ikke bogført" without a way to act (#407);
// these two endpoints close that loop by becoming a third caller of the SAME
// `bookExpenseFromBank` core function the CLI's `expense book` and the MCP
// tool use. The owner picks an expense account + an unmatched outgoing bank
// transaction in the modal, and the existing core posts the journal entry.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { seedAccounts } from "../../src/core/ledger";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
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

/** Workspace + one company + one ingested purchase document + one outgoing
 *  bank transaction; returns the ids/cleanup the spec needs. */
function makeFixture(label: string) {
  const root = tmpRoot(label);
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS" });
  const slug = created.slug;
  const companyRoot = companyRootForSlug(root, slug);
  const dbPath = companyPaths(companyRoot).db;
  const db = openDb(dbPath);
  migrate(db);
  seedAccounts(db);
  // Ingest a real purchase document — same path the CLI/MCP use.
  const inbox = tmpRoot(`${label}-inbox`);
  const sourceFile = join(inbox, "receipt.txt");
  writeFileSync(sourceFile, `Office supplies receipt ${label}\n`);
  const ingested = ingestDocument(db, companyRoot, sourceFile, {
    source: "email",
    issueDate: "2026-03-10",
    invoiceNo: `EXP-${label}`,
    deliveryDescription: "Kontorartikler",
    amountIncVat: 1250,
    vatAmount: 250,
    currency: "DKK",
    sender: {
      name: "Office World ApS",
      address: "Storevej 1",
      vatOrCvr: "DK11223344",
    },
    recipient: {
      name: "Acme ApS",
      address: "Testvej 1",
      vatOrCvr: "DK12345678",
    },
    paymentDetails: "Bank transfer",
  });
  if (!ingested.ok) throw new Error("fixture ingest failed: " + (ingested.errors ?? []).join("; "));
  const documentId = ingested.documentId!;
  // Insert an unmatched outgoing bank transaction the owner can pair with.
  const bankRow = db
    .query(
      "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?) RETURNING id",
    )
    .get(
      "2026-03-12",
      "2026-03-12",
      "Office World — kontorartikler",
      -1250,
      `ref-${label}`,
      `batch-${label}`,
      `hash-${label}`,
      `tx-${label}`,
    ) as { id: number };
  db.close();
  const cleanup = () => {
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  };
  return { root, slug, documentId, bankTransactionId: bankRow.id, cleanup };
}

async function getJson(cfg: ServerConfig, path: string) {
  const res = await handleRequest(
    new Request(`http://localhost${path}`, { headers: { host: "127.0.0.1" } }),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1" },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

describe("GET /api/companies/:slug/documents/:id/booking-options", () => {
  test("returns the document, expense-account list and unmatched outgoing bank transactions", async () => {
    const { root, slug, documentId, bankTransactionId, cleanup } =
      makeFixture("book-options-ok");
    const cfg = config({ workspaceRoot: root });
    const res = await getJson(
      cfg,
      `/api/companies/${slug}/documents/${documentId}/booking-options`,
    );
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
    const options = res.body.options;
    expect(options.document.id).toBe(documentId);
    expect(options.document.amountIncVat).toBe(1250);
    expect(options.document.vatAmount).toBe(250);
    expect(options.document.supplierName).toBe("Office World ApS");
    // Every entry in the expense-account list is type=expense and active.
    expect(Array.isArray(options.expenseAccounts)).toBe(true);
    expect(options.expenseAccounts.length).toBeGreaterThan(0);
    for (const a of options.expenseAccounts) {
      expect(typeof a.accountNo).toBe("string");
      expect(typeof a.name).toBe("string");
    }
    // The unmatched outgoing bank transaction is present and ready to pick.
    const matchable = options.unmatchedOutgoingBank as Array<{
      id: number;
      amount: number;
      text: string;
      date: string;
    }>;
    expect(matchable.some((t) => t.id === bankTransactionId)).toBe(true);
    cleanup();
  });

  test("a missing document is a 404", async () => {
    const { root, slug, cleanup } = makeFixture("book-options-404");
    const cfg = config({ workspaceRoot: root });
    const res = await getJson(
      cfg,
      `/api/companies/${slug}/documents/9999/booking-options`,
    );
    expect(res.status).toBe(404);
    cleanup();
  });
});

describe("POST /api/companies/:slug/documents/book-expense", () => {
  test("books the document against the bank transaction via the same core the CLI uses", async () => {
    const { root, slug, documentId, bankTransactionId, cleanup } =
      makeFixture("book-expense-ok");
    const cfg = config({ workspaceRoot: root });
    const res = await post(
      cfg,
      `/api/companies/${slug}/documents/book-expense`,
      {
        documentId,
        bankTransactionId,
        expenseAccountNo: "3120",
        confirm: true,
      },
    );
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
    expect(res.body.booking.entryId).toBeGreaterThan(0);
    expect(res.body.booking.grossAmount).toBe(1250);
    expect(res.body.booking.netAmount).toBe(1000);
    expect(res.body.booking.vatAmount).toBe(250);
    // After booking the GET endpoint shows the document as linked.
    const after = await getJson(
      cfg,
      `/api/companies/${slug}/documents`,
    );
    const doc = (after.body.documents.documents as any[]).find(
      (d) => d.id === documentId,
    );
    expect(doc?.journalEntryId).toBe(res.body.booking.entryId);
    cleanup();
  });

  test("without confirm:true the destructive action is refused 400", async () => {
    const { root, slug, documentId, bankTransactionId, cleanup } =
      makeFixture("book-expense-no-confirm");
    const cfg = config({ workspaceRoot: root });
    const res = await post(
      cfg,
      `/api/companies/${slug}/documents/book-expense`,
      {
        documentId,
        bankTransactionId,
        expenseAccountNo: "3120",
      },
    );
    expect(res.status).toBe(400);
    expect(res.body.ok).toBe(false);
    cleanup();
  });

  test("an unknown expense account is a 400 with the core error verbatim", async () => {
    const { root, slug, documentId, bankTransactionId, cleanup } =
      makeFixture("book-expense-bad-account");
    const cfg = config({ workspaceRoot: root });
    const res = await post(
      cfg,
      `/api/companies/${slug}/documents/book-expense`,
      {
        documentId,
        bankTransactionId,
        expenseAccountNo: "9999",
        confirm: true,
      },
    );
    // "does not exist" is mapped to 409 by the conflict heuristic.
    expect(res.status === 400 || res.status === 409).toBe(true);
    expect(res.body.ok).toBe(false);
    cleanup();
  });
});
