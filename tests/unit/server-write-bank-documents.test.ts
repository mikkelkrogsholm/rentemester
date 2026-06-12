// Tests: src/server/write-handlers.ts (handleBankImport, handleDocumentIngest)
// and the POST .../bank/import + .../documents/ingest routes in
// src/server/router.ts (#213, slices 2-3).
//
// Both routes are file-upload write actions routed through the shared
// `withCompanyMutation` pipeline. These specs cover the cross-cutting gates
// (backup lock, actor attribution, localhost hard-gate, confirm gate, max
// body size) and the concrete bank-import / document-ingest actions end to
// end against a real company ledger.
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

/** Inserts a bank transaction so a weekly backup becomes "due". */
function insertBankActivity(ws: string, slug: string, date: string, ref: string): void {
  withLedger(ws, slug, (db) => {
    db.run(
      "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
      date,
      date,
      "Activity",
      500,
      ref,
      `batch-${ref}`,
      `hash-${ref}`,
      `tx-${ref}`,
    );
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

const BANK_CSV = [
  "transaction_date,booking_date,text,amount,currency,reference",
  "2026-05-16,2026-05-17,Card payment,-1250,DKK,REF-1",
  "2026-05-18,2026-05-18,Customer payment,2500,DKK,REF-2",
].join("\n");

// --------------------------------------------------------------------------
// Slice 2 — bank CSV import
// --------------------------------------------------------------------------

describe("Cockpit write — bank import (happy path)", () => {
  test("a POST .../bank/import imports the CSV rows and reports ok", async () => {
    const { root: ws, slug } = makeWorkspace("bank-ok");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV, confirm: true },
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.import.imported).toBe(2);
      expect(res.body.import.skippedDuplicates).toBe(0);

      // The two rows now live in the ledger.
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT COUNT(*) AS n FROM bank_transactions")
          .get() as { n: number };
        expect(row.n).toBe(2);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a second import of the same CSV skips deterministic duplicates", async () => {
    const { root: ws, slug } = makeWorkspace("bank-dup");
    try {
      const cfg = config({ workspaceRoot: ws });
      await post(cfg, `/api/companies/${slug}/bank/import`, {
        csvContent: BANK_CSV,
        confirm: true,
      });
      const res = await post(cfg, `/api/companies/${slug}/bank/import`, {
        csvContent: BANK_CSV,
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.import.imported).toBe(0);
      expect(res.body.import.skippedDuplicates).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("import creates unmatched-transaction exceptions", async () => {
    const { root: ws, slug } = makeWorkspace("bank-exc");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV, confirm: true },
      );
      expect(res.status).toBe(200);
      expect(res.body.import.exceptionsCreated).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — bank import (gates + input errors)", () => {
  test("without confirm:true the write is refused with 400", async () => {
    const { root: ws, slug } = makeWorkspace("bank-noconfirm");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV },
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      // Nothing was imported.
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT COUNT(*) AS n FROM bank_transactions")
          .get() as { n: number };
        expect(row.n).toBe(0);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing csvContent is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("bank-nocsv");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { confirm: true },
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown bank profile is mapped to a 400, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("bank-badprofile");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV, profile: "no-such-bank", confirm: true },
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      expect(res.body.errors[0]).toContain("no-such-bank");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a body over the upload limit is refused with 400", async () => {
    const { root: ws, slug } = makeWorkspace("bank-toobig");
    try {
      // 13 MiB of declared content-length — over the 12 MiB cap.
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/bank/import`, {
          method: "POST",
          headers: {
            host: "127.0.0.1",
            "content-length": String(13 * 1024 * 1024),
          },
          body: JSON.stringify({ csvContent: BANK_CSV, confirm: true }),
        }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(400);
      const body = (await res.json()) as { errors: string[] };
      // Post-#242 the size-guard message is in Danish ("…overskrider grænsen…").
      // The previous assertion pinned the English token "limit".
      expect(body.errors[0]).toMatch(/overskrider grænsen|bytes/i);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("bank-noco");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/nope-aps/bank/import`,
        { csvContent: BANK_CSV, confirm: true },
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the bank-import route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("bank-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/bank/import`, {
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

describe("Cockpit write — bank import (backup lock + localhost gate)", () => {
  test("a bank import is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("bank-locked");
    try {
      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, {
          createdAt: new Date(Date.now() - 10 * DAY).toISOString(),
        });
      });
      insertBankActivity(
        ws,
        slug,
        new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10),
        "late",
      );
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV, confirm: true },
      );
      expect(res.status).toBe(409);
      expect(res.body.code).toBe("conflict");
      expect(res.body.errors[0]).toContain("Bogføring er låst");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a bank import from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("bank-remotehost");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/bank/import`,
        { csvContent: BANK_CSV, confirm: true },
        { host: "cockpit.example.com" },
      );
      expect(res.status).toBe(401);
      expect(res.body.code).toBe("unauthorized");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// --------------------------------------------------------------------------
// Slice 3 — document (bilag) intake
// --------------------------------------------------------------------------

/** A minimal valid cash-register-receipt body — a plain-text file ingests. */
function receiptBody(over: Record<string, unknown> = {}) {
  return {
    fileName: "kvittering.txt",
    fileBase64: Buffer.from("Kasseboner\n12,00 DKK\n", "utf8").toString("base64"),
    metadata: {
      source: "photo-upload",
      documentType: "cash_register_receipt",
      currency: "DKK",
    },
    confirm: true,
    ...over,
  };
}

describe("Cockpit write — document ingest (happy path)", () => {
  test("a POST .../documents/ingest stores the bilag and reports ok", async () => {
    const { root: ws, slug } = makeWorkspace("doc-ok");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody(),
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(typeof res.body.document.id).toBe("number");
      expect(res.body.document.documentNo).toMatch(/^DOC-/);

      // The document row is in the ledger.
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT COUNT(*) AS n FROM documents")
          .get() as { n: number };
        expect(row.n).toBe(1);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — document ingest (gates + input errors)", () => {
  test("without confirm:true the ingest is refused with 400", async () => {
    const { root: ws, slug } = makeWorkspace("doc-noconfirm");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody({ confirm: undefined }),
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT COUNT(*) AS n FROM documents")
          .get() as { n: number };
        expect(row.n).toBe(0);
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing fileBase64 is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("doc-nofile");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody({ fileBase64: undefined }),
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing metadata.source is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("doc-nosource");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody({ metadata: { documentType: "cash_register_receipt" } }),
      );
      expect(res.status).toBe(400);
      expect(res.body.errors[0]).toContain("source");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("invalid purchase/sale metadata is mapped to a 400, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("doc-badmeta");
    try {
      // purchase_sale needs the full statutory field set — this lacks it.
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody({
          metadata: { source: "email", documentType: "purchase_sale" },
        }),
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      expect(res.body.errors[0]).toContain("required");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown vendorId is mapped to a conflict, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("doc-badvendor");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody({ vendorId: 9999 }),
      );
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("vendor");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a document ingest from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("doc-remotehost");
    try {
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody(),
        { host: "cockpit.example.com" },
      );
      expect(res.status).toBe(401);
      expect(res.body.code).toBe("unauthorized");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a document ingest is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("doc-locked");
    try {
      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, {
          createdAt: new Date(Date.now() - 10 * DAY).toISOString(),
        });
      });
      insertBankActivity(
        ws,
        slug,
        new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10),
        "late",
      );
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/documents/ingest`,
        receiptBody(),
      );
      expect(res.status).toBe(409);
      expect(res.body.errors[0]).toContain("Bogføring er låst");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the document-ingest route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("doc-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/documents/ingest`, {
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
