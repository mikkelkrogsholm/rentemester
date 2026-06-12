// Tests: src/server/write-handlers.ts (handlePayableRegister, handlePayablePay)
// + src/server/data/payables-view.ts (buildCompanyPayables) and the
// GET/POST .../payables routes in src/server/router.ts (#340).
//
// The cockpit's Leverandørfaktura-arbejdsbord is the third caller of
// `core/payables.ts`, alongside the CLI `payable register/pay` commands.
// These specs cover the cross-cutting gates (auth, confirm, method), the happy
// path, and the input/business-rejection error mapping — including the
// duplicate-registration and double-pay 409 conflicts.

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
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyName = "Acme ApS") {
  const root = tmpRoot(label);
  initWorkspace(root);
  const created = createCompany(root, { name: companyName });
  return { root, slug: created.slug };
}

function config(
  overrides: Partial<ServerConfig> & { workspaceRoot: string },
): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    ...overrides,
  };
}

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

/**
 * Ingests a purchase document via the same `ingestDocument` core function the
 * CLI uses. Returns the document id ready to register as a payable.
 */
function ingestPurchase(
  ws: string,
  slug: string,
  inboxDir: string,
  supplierName: string,
  invoiceNo: string,
  amountIncVat: number,
  vatAmount: number,
): number {
  const sourceFile = join(inboxDir, `${invoiceNo}.txt`);
  writeFileSync(sourceFile, `Invoice ${invoiceNo}\n${amountIncVat} DKK\n`);
  return withLedger(ws, slug, (db, companyRoot) => {
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: "2026-01-10",
      invoiceNo,
      deliveryDescription: "Leverandørydelse",
      amountIncVat,
      currency: "DKK",
      sender: {
        name: supplierName,
        address: "Leverandørvej 1",
        vatOrCvr: "DK11223344",
      },
      recipient: {
        name: "Acme ApS",
        address: "Testvej 1",
        vatOrCvr: "DK12345678",
      },
      vatAmount,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);
    return doc.documentId!;
  });
}

/** Imports an outgoing supplier payment for the given amount. Returns its id. */
function importBankOut(
  ws: string,
  slug: string,
  date: string,
  text: string,
  ref: string,
  amount: number,
): number {
  const companyRoot = companyRootForSlug(ws, slug);
  const csv = join(companyRoot, `bank-${ref}.csv`);
  writeFileSync(
    csv,
    [
      "transaction_date,booking_date,text,amount,currency,reference",
      `${date},${date},${text},${amount},DKK,${ref}`,
    ].join("\n"),
  );
  return withLedger(ws, slug, (db) => {
    const res = importBankCsv(db, companyRoot, csv);
    expect(res.ok).toBe(true);
    const row = db
      .query(
        `SELECT id FROM bank_transactions WHERE reference = ? ORDER BY id DESC LIMIT 1`,
      )
      .get(ref) as { id: number };
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
  const res = await handleRequest(
    new Request(`http://localhost${path}`, init),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

async function get(cfg: ServerConfig, path: string) {
  const res = await handleRequest(
    new Request(`http://localhost${path}`, {
      method: "GET",
      headers: { host: "127.0.0.1" },
    }),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

// --------------------------------------------------------------------------
// GET /api/companies/:slug/payables — kreditorliste + modal-data
// --------------------------------------------------------------------------

describe("Cockpit read — leverandørfaktura-arbejdsbordet", () => {
  test("an empty company returns an empty kreditorliste with picker rows", async () => {
    const { root: ws, slug } = makeWorkspace("payable-list-empty");
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const view = res.body.payables;
      expect(view.slug).toBe(slug);
      expect(view.rows).toEqual([]);
      expect(view.count).toBe(0);
      expect(view.totalOpenBalance).toBe(0);
      // The seeded chart of accounts gives the modal a non-empty picker.
      expect(Array.isArray(view.expenseAccounts)).toBe(true);
      expect(view.expenseAccounts.length).toBeGreaterThan(0);
      expect(view.unregisteredDocuments).toEqual([]);
      expect(view.vendors).toEqual([]);
      // The status filter defaults to "open" so the action-needed list is
      // surfaced by default — the same default as the CLI's `payable list`.
      expect(view.status).toBe("open");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an ingested purchase document shows up as unregistered, and a registered payable shows up in the list", async () => {
    const { root: ws, slug } = makeWorkspace("payable-list-rows");
    const inbox = tmpRoot("payable-list-rows-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-1001",
        1250,
        250,
      );

      // Before registration: the document is in the unregistered-modal-picker.
      let view = (
        await get(
          config({ workspaceRoot: ws }),
          `/api/companies/${slug}/payables`,
        )
      ).body.payables;
      expect(view.unregisteredDocuments.length).toBe(1);
      expect(view.unregisteredDocuments[0].id).toBe(documentId);
      expect(view.unregisteredDocuments[0].invoiceNo).toBe("V-1001");
      expect(view.unregisteredDocuments[0].amountIncVat).toBe(1250);
      expect(view.unregisteredDocuments[0].vatAmount).toBe(250);
      expect(view.rows).toEqual([]);

      // Register it.
      const reg = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          documentId,
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          confirm: true,
        },
      );
      expect(reg.status).toBe(200);
      expect(reg.body.ok).toBe(true);
      expect(reg.body.payable.payableId).toBeGreaterThan(0);

      // After registration: a row appears, no longer in unregistered.
      view = (
        await get(
          config({ workspaceRoot: ws }),
          `/api/companies/${slug}/payables?status=all`,
        )
      ).body.payables;
      expect(view.unregisteredDocuments).toEqual([]);
      expect(view.rows.length).toBe(1);
      expect(view.rows[0].documentId).toBe(documentId);
      expect(view.rows[0].billNo).toBe("V-1001");
      expect(view.rows[0].grossAmount).toBe(1250);
      expect(view.rows[0].openBalance).toBe(1250);
      expect(view.rows[0].status).toBe("open");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("payable-list-noco");
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        `/api/companies/no-such-aps/payables`,
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unsupported method on the list route is a 405", async () => {
    const { root: ws, slug } = makeWorkspace("payable-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/payables`, {
          method: "PATCH",
          headers: { host: "127.0.0.1" },
        }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// POST /api/companies/:slug/payables — registrér
// --------------------------------------------------------------------------

describe("Cockpit write — payable register", () => {
  test("registers an ingested purchase document and posts the kreditorpost", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-ok");
    const inbox = tmpRoot("payable-register-ok-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-2001",
        1250,
        250,
      );
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          documentId,
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          confirm: true,
        },
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const payable = res.body.payable;
      expect(payable.payableId).toBeGreaterThan(0);
      expect(payable.grossAmount).toBe(1250);
      expect(payable.netAmount).toBe(1000);
      expect(payable.vatAmount).toBe(250);
      expect(payable.dueDate).toBe("2026-02-09");
      expect(payable.entryId).toBeGreaterThan(0);

      // The journal entry is balanced and references the document.
      withLedger(ws, slug, (db) => {
        const row = db
          .query(
            `SELECT COUNT(*) AS n FROM journal_entries WHERE document_id = ?`,
          )
          .get(documentId) as { n: number };
        expect(row.n).toBe(1);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("without confirm:true the register is refused with 400 and nothing posts", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-noconfirm");
    const inbox = tmpRoot("payable-register-noconfirm-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-3001",
        1250,
        250,
      );
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          documentId,
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          // no confirm
        },
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      withLedger(ws, slug, (db) => {
        const row = db
          .query(`SELECT COUNT(*) AS n FROM payables`)
          .get() as { n: number };
        expect(row.n).toBe(0);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("a missing documentId is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-nodoc");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          confirm: true,
        },
      );
      expect(res.status).toBe(400);
      expect((res.body.errors?.[0] ?? "")).toContain("documentId");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid vatTreatment value is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-badvat");
    const inbox = tmpRoot("payable-register-badvat-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-4001",
        1250,
        250,
      );
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          documentId,
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          vatTreatment: "reverse_charge",
          confirm: true,
        },
      );
      expect(res.status).toBe(400);
      expect((res.body.errors?.[0] ?? "")).toContain("vatTreatment");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("registering the same document twice is mapped to a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-dup");
    const inbox = tmpRoot("payable-register-dup-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-5001",
        1250,
        250,
      );
      const cfg = config({ workspaceRoot: ws });
      const first = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      expect(first.status).toBe(200);
      const second = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      expect(second.status).toBe(409);
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("a register from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("payable-register-remote");
    const inbox = tmpRoot("payable-register-remote-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-6001",
        1250,
        250,
      );
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables`,
        {
          documentId,
          billDate: "2026-01-10",
          dueDate: "2026-02-09",
          expenseAccountNo: "3000",
          confirm: true,
        },
        { host: "cockpit.example.com" },
      );
      expect(res.status).toBe(401);
      expect(res.body.code).toBe("unauthorized");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// POST /api/companies/:slug/payables/:id/pay — match outgoing bank payment
// --------------------------------------------------------------------------

describe("Cockpit write — payable pay", () => {
  test("matches an outgoing bank payment against an open payable and closes it", async () => {
    const { root: ws, slug } = makeWorkspace("payable-pay-ok");
    const inbox = tmpRoot("payable-pay-ok-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-7001",
        1250,
        250,
      );
      const bankTxId = importBankOut(
        ws,
        slug,
        "2026-02-05",
        "SOFTWARE APS",
        "REF-PAY-1",
        -1250,
      );
      const cfg = config({ workspaceRoot: ws });
      const reg = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      expect(reg.status).toBe(200);
      const payableId = reg.body.payable.payableId as number;

      const pay = await post(
        cfg,
        `/api/companies/${slug}/payables/${payableId}/pay`,
        {
          bankTransactionId: bankTxId,
          confirm: true,
        },
      );
      expect(pay.status).toBe(200);
      expect(pay.body.ok).toBe(true);
      expect(pay.body.payment.payableId).toBe(payableId);
      expect(pay.body.payment.openBalance).toBe(0);
      expect(pay.body.payment.paymentId).toBeGreaterThan(0);
      expect(pay.body.payment.journalEntryId).toBeGreaterThan(0);

      // The list now shows the payable as paid.
      const list = (
        await get(cfg, `/api/companies/${slug}/payables?status=paid`)
      ).body.payables;
      expect(list.rows.length).toBe(1);
      expect(list.rows[0].status).toBe("paid");
      expect(list.rows[0].openBalance).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("without confirm:true the pay is refused with 400 and nothing posts", async () => {
    const { root: ws, slug } = makeWorkspace("payable-pay-noconfirm");
    const inbox = tmpRoot("payable-pay-noconfirm-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-7101",
        1250,
        250,
      );
      const bankTxId = importBankOut(
        ws,
        slug,
        "2026-02-05",
        "SOFTWARE APS",
        "REF-PAY-2",
        -1250,
      );
      const cfg = config({ workspaceRoot: ws });
      const reg = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      const payableId = reg.body.payable.payableId as number;
      const pay = await post(
        cfg,
        `/api/companies/${slug}/payables/${payableId}/pay`,
        { bankTransactionId: bankTxId },
      );
      expect(pay.status).toBe(400);
      expect(pay.body.code).toBe("bad_request");
      withLedger(ws, slug, (db) => {
        const row = db
          .query(`SELECT COUNT(*) AS n FROM payable_payments`)
          .get() as { n: number };
        expect(row.n).toBe(0);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("a missing bankTransactionId is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("payable-pay-nobank");
    const inbox = tmpRoot("payable-pay-nobank-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-7201",
        1250,
        250,
      );
      const cfg = config({ workspaceRoot: ws });
      const reg = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      const payableId = reg.body.payable.payableId as number;
      const pay = await post(
        cfg,
        `/api/companies/${slug}/payables/${payableId}/pay`,
        { confirm: true },
      );
      expect(pay.status).toBe(400);
      expect((pay.body.errors?.[0] ?? "")).toContain("bankTransactionId");
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("paying the same bank line twice is mapped to a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("payable-pay-dup");
    const inbox = tmpRoot("payable-pay-dup-inbox");
    try {
      const documentId = ingestPurchase(
        ws,
        slug,
        inbox,
        "Software ApS",
        "V-7301",
        1250,
        250,
      );
      const bankTxId = importBankOut(
        ws,
        slug,
        "2026-02-05",
        "SOFTWARE APS",
        "REF-PAY-3",
        -1250,
      );
      const cfg = config({ workspaceRoot: ws });
      const reg = await post(cfg, `/api/companies/${slug}/payables`, {
        documentId,
        billDate: "2026-01-10",
        dueDate: "2026-02-09",
        expenseAccountNo: "3000",
        confirm: true,
      });
      const payableId = reg.body.payable.payableId as number;
      const first = await post(
        cfg,
        `/api/companies/${slug}/payables/${payableId}/pay`,
        { bankTransactionId: bankTxId, confirm: true },
      );
      expect(first.status).toBe(200);
      const second = await post(
        cfg,
        `/api/companies/${slug}/payables/${payableId}/pay`,
        { bankTransactionId: bankTxId, confirm: true },
      );
      expect(second.status).toBe(409);
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("a bad payable id in the path is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("payable-pay-badid");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/payables/0/pay`,
        { bankTransactionId: 1, confirm: true },
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
