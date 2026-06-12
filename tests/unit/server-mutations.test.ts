// Tests: src/server/mutations.ts, src/server/actor.ts, src/server/write-handlers.ts
// and the POST .../exceptions/:id/resolve route in src/server/router.ts (#213,
// slice 1).
//
// Covers the shared Cockpit write pipeline: the backup-lock gate, actor
// attribution, the localhost hard-gate, and the concrete resolve-exception
// action end-to-end.
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
import { recordException } from "../../src/core/exceptions";
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
function withLedger<T>(ws: string, slug: string, fn: (db: ReturnType<typeof openDb>, companyRoot: string) => T): T {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    return fn(db, companyRoot);
  } finally {
    db.close();
  }
}

/** Records one open exception directly in a company ledger; returns its id. */
function seedException(ws: string, slug: string, message = "Banktransaktion mangler afstemning"): number {
  return withLedger(ws, slug, (db) => {
    const result = recordException(db, { type: "UNMATCHED_BANK_TRANSACTION", severity: "medium", message });
    return result.exceptionId!;
  });
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

async function post(cfg: ServerConfig, path: string, body?: unknown, headers?: Record<string, string>) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1", ...(headers ?? {}) },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

describe("Cockpit write — resolve exception (happy path)", () => {
  test("a POST .../resolve clears an open exception and reports ok", async () => {
    const { root: ws, slug } = makeWorkspace("mut-ok");
    try {
      const id = seedException(ws, slug);
      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/${id}/resolve`, {
        note: "Afstemt manuelt",
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.exception).toEqual({ id, resolved: true });

      // The exception row is now resolved, with the note persisted.
      withLedger(ws, slug, (db) => {
        const row = db.query("SELECT status, resolution_note FROM exceptions WHERE id = ?").get(id) as {
          status: string;
          resolution_note: string | null;
        };
        expect(row.status).toBe("resolved");
        expect(row.resolution_note).toBe("Afstemt manuelt");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("works with no request body — the note is optional", async () => {
    const { root: ws, slug } = makeWorkspace("mut-nobody");
    try {
      const id = seedException(ws, slug);
      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/${id}/resolve`);
      expect(res.status).toBe(200);
      expect(res.body.exception.resolved).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("resolving an already-resolved exception is idempotent (ok, resolved:false)", async () => {
    const { root: ws, slug } = makeWorkspace("mut-idem");
    try {
      const id = seedException(ws, slug);
      const cfg = config({ workspaceRoot: ws });
      await post(cfg, `/api/companies/${slug}/exceptions/${id}/resolve`);
      const res = await post(cfg, `/api/companies/${slug}/exceptions/${id}/resolve`);
      expect(res.status).toBe(200);
      expect(res.body.exception.resolved).toBe(false);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — actor attribution", () => {
  test("a Cockpit-resolved exception is attributed to the fixed web actor", async () => {
    const { root: ws, slug } = makeWorkspace("mut-actor");
    try {
      const id = seedException(ws, slug);
      await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/${id}/resolve`);
      // resolved_by is the canonical web actor id, set via an explicit payload
      // param — never an env var.
      withLedger(ws, slug, (db) => {
        const row = db.query("SELECT resolved_by FROM exceptions WHERE id = ?").get(id) as {
          resolved_by: string | null;
        };
        expect(row.resolved_by).toBe("system:cockpit");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — backup-lock gate", () => {
  test("a write is refused with 409 when the backup lock is engaged", async () => {
    const { root: ws, slug } = makeWorkspace("mut-locked");
    try {
      const id = seedException(ws, slug);
      // Make a weekly backup overdue past the grace window, then enforce.
      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, { createdAt: new Date(Date.now() - 10 * DAY).toISOString() });
      });
      insertBankActivity(ws, slug, new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10), "late");
      withLedger(ws, slug, (db, companyRoot) => {
        configureBackupLock(db, companyRoot, { enforced: true, graceDays: 0 });
      });

      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/${id}/resolve`);
      expect(res.status).toBe(409);
      expect(res.body.ok).toBe(false);
      expect(res.body.code).toBe("conflict");
      expect(res.body.errors[0]).toContain("Bogføring er låst");
      expect(res.body.errors[0]).toContain("BEK 205/2024");

      // The exception must remain open — the locked write did nothing.
      withLedger(ws, slug, (db) => {
        const row = db.query("SELECT status FROM exceptions WHERE id = ?").get(id) as { status: string };
        expect(row.status).toBe("open");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a write is allowed when enforcement is opt-out (the default)", async () => {
    const { root: ws, slug } = makeWorkspace("mut-unlocked");
    try {
      const id = seedException(ws, slug);
      // Backup overdue, but enforcement is OFF — the lock must not engage.
      withLedger(ws, slug, (db, companyRoot) => {
        createSystemBackup(db, companyRoot, { createdAt: new Date(Date.now() - 10 * DAY).toISOString() });
      });
      insertBankActivity(ws, slug, new Date(Date.now() - 2 * DAY).toISOString().slice(0, 10), "late");

      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/${id}/resolve`);
      expect(res.status).toBe(200);
      expect(res.body.exception.resolved).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — input + routing errors", () => {
  test("an unknown exception id is a 409 conflict, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("mut-missing");
    try {
      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/9999/resolve`);
      expect(res.status).toBe(409);
      expect(res.body.code).toBe("conflict");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-integer id segment is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("mut-badid");
    try {
      const res = await post(config({ workspaceRoot: ws }), `/api/companies/${slug}/exceptions/abc/resolve`);
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("mut-noco");
    try {
      const res = await post(config({ workspaceRoot: ws }), `/api/companies/nope-aps/exceptions/1/resolve`);
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a malformed JSON body is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("mut-badjson");
    try {
      const id = seedException(ws, slug);
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/exceptions/${id}/resolve`, {
          method: "POST",
          headers: { host: "127.0.0.1" },
          body: "{not json",
        }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the resolve route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("mut-method");
    try {
      const res = await handleRequest(
        new Request(`http://localhost/api/companies/${slug}/exceptions/1/resolve`, { method: "GET" }),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit write — localhost hard-gate", () => {
  test("a write from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("mut-remotehost");
    try {
      const id = seedException(ws, slug);
      const res = await post(
        config({ workspaceRoot: ws }),
        `/api/companies/${slug}/exceptions/${id}/resolve`,
        undefined,
        { host: "cockpit.example.com" },
      );
      expect(res.status).toBe(401);
      expect(res.body.code).toBe("unauthorized");
      // The exception must remain open.
      withLedger(ws, slug, (db) => {
        const row = db.query("SELECT status FROM exceptions WHERE id = ?").get(id) as { status: string };
        expect(row.status).toBe("open");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a write from a non-loopback host is allowed once auth (a token) is required", async () => {
    const { root: ws, slug } = makeWorkspace("mut-remoteauth");
    try {
      const id = seedException(ws, slug);
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await post(cfg, `/api/companies/${slug}/exceptions/${id}/resolve`, undefined, {
        host: "cockpit.example.com",
        authorization: "Bearer s3cret",
      });
      expect(res.status).toBe(200);
      expect(res.body.exception.resolved).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a write with auth required but no token is rejected at the auth seam", async () => {
    const { root: ws, slug } = makeWorkspace("mut-noauth");
    try {
      const id = seedException(ws, slug);
      const cfg = config({ workspaceRoot: ws, authRequired: true, authToken: "s3cret" });
      const res = await post(cfg, `/api/companies/${slug}/exceptions/${id}/resolve`, undefined, {
        host: "127.0.0.1",
      });
      expect(res.status).toBe(401);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
