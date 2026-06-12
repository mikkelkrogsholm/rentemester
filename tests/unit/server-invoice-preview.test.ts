// Tests: src/server/write-handlers.ts (handleInvoicePreview) and the POST
// /api/companies/:slug/invoices/preview route in src/server/router.ts (#440).
//
// #440 introduces a read-only "Forhåndsvis" endpoint so the cockpit can show
// the customer-facing PDF BEFORE the irreversible issue posting. The endpoint
// shares the input validation + master-data resolution shape with
// `/invoices/issue`, but MUST NOT mutate anything: no sequence draw, no
// documents row, no audit_log entry. These specs lock that contract.
//
// The endpoint returns the raw PDF bytes (Content-Type application/pdf) so
// the cockpit can pipe them straight into URL.createObjectURL(); the tests
// inspect the response bytes + headers and verify the ledger is untouched.
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

function withLedger<T>(
  ws: string,
  slug: string,
  fn: (db: ReturnType<typeof openDb>) => T,
): T {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    return fn(db);
  } finally {
    db.close();
  }
}

/** Same shape as `issueBody` in server-write-invoices.test.ts — one line, 25%. */
function previewBody(over: Record<string, unknown> = {}) {
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

/**
 * Calls the preview endpoint with the given body. Returns the raw Response so
 * tests can introspect headers + bytes (the response is binary PDF, not the
 * JSON envelope the write routes return on success).
 */
async function postPreview(
  cfg: ServerConfig,
  slug: string,
  body?: unknown,
  headers?: Record<string, string>,
) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1", ...(headers ?? {}) },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  return await handleRequest(
    new Request(
      `http://localhost/api/companies/${slug}/invoices/preview`,
      init,
    ),
    cfg,
  );
}

describe("Cockpit read+render — invoice preview (happy path)", () => {
  test("POST .../invoices/preview returns the customer-facing PDF bytes", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-ok");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody(),
      );
      expect(res.status).toBe(200);
      expect(res.headers.get("content-type")).toBe("application/pdf");
      // Inline disposition with a UDKAST-tagged filename — the cockpit opens
      // the response in a new tab via URL.createObjectURL, so the disposition
      // must NOT be `attachment`.
      const disp = res.headers.get("content-disposition") ?? "";
      expect(disp).toContain("inline");
      expect(disp.toUpperCase()).toContain("UDKAST");
      // Cache must NOT be public — a preview can leak draft amounts/customers.
      expect(res.headers.get("cache-control")).toContain("no-store");
      // Bytes look like a PDF (start with %PDF magic header).
      const bytes = new Uint8Array(await res.arrayBuffer());
      expect(bytes.length).toBeGreaterThan(100);
      const magic = String.fromCharCode(
        bytes[0]!,
        bytes[1]!,
        bytes[2]!,
        bytes[3]!,
      );
      expect(magic).toBe("%PDF");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // The single load-bearing assertion for #440: a preview must NOT mutate the
  // ledger. Sequence stays at 0, no documents row, no audit_log entry.
  test("preview does NOT touch the sequence, documents or audit_log", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-noside");
    try {
      const before = withLedger(ws, slug, (db) => ({
        docs: (db
          .query("SELECT COUNT(*) AS n FROM documents")
          .get() as { n: number }).n,
        audit: (db
          .query("SELECT COUNT(*) AS n FROM audit_log")
          .get() as { n: number }).n,
        seq: (db
          .query(
            "SELECT COALESCE(SUM(value), 0) AS v FROM sequences WHERE kind = 'invoice'",
          )
          .get() as { v: number }).v,
      }));

      // Run the preview twice — duplicate calls must remain side-effect free.
      const res1 = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody(),
      );
      expect(res1.status).toBe(200);
      const res2 = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({ issueDate: "2026-05-17" }),
      );
      expect(res2.status).toBe(200);

      const after = withLedger(ws, slug, (db) => ({
        docs: (db
          .query("SELECT COUNT(*) AS n FROM documents")
          .get() as { n: number }).n,
        audit: (db
          .query("SELECT COUNT(*) AS n FROM audit_log")
          .get() as { n: number }).n,
        seq: (db
          .query(
            "SELECT COALESCE(SUM(value), 0) AS v FROM sequences WHERE kind = 'invoice'",
          )
          .get() as { v: number }).v,
      }));

      expect(after.docs).toBe(before.docs);
      expect(after.audit).toBe(before.audit);
      expect(after.seq).toBe(before.seq);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // The other half of #440's contract: an issue AFTER a preview still gets
  // the expected first sequence number (no hole left by the preview).
  test("a real issue AFTER a preview still draws the first sequence number", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-thenissue");
    try {
      // 1) Two previews — should burn no sequence numbers.
      await postPreview(config({ workspaceRoot: ws }), slug, previewBody());
      await postPreview(config({ workspaceRoot: ws }), slug, previewBody());

      // 2) A real issue — should still get the first fortløbende number.
      const issueRes = await handleRequest(
        new Request(
          `http://localhost/api/companies/${slug}/invoices/issue`,
          {
            method: "POST",
            headers: { host: "127.0.0.1" },
            body: JSON.stringify(previewBody()),
          },
        ),
        config({ workspaceRoot: ws }),
      );
      expect(issueRes.status).toBe(200);
      const issueBody = await issueRes.json();
      expect(issueBody.ok).toBe(true);
      // The first issued invoice of the fiscal year must end in -0001 — the
      // previews must not have left a hole.
      expect(issueBody.invoice.invoiceNumber).toMatch(/-0001$/);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("Cockpit read+render — invoice preview (input + gates)", () => {
  test("missing issueDate is a 400 with the cockpit error envelope", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-nodate");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({ issueDate: undefined }),
      );
      expect(res.status).toBe(400);
      // Validation errors come back as the regular JSON envelope so the
      // cockpit can surface them in the same red banner as Udsted would.
      expect(res.headers.get("content-type") ?? "").toContain("application/json");
      const body = await res.json();
      expect(body.ok).toBe(false);
      expect(body.code).toBe("bad_request");
      expect((body.errors?.[0] ?? "")).toContain("issueDate");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty lines array is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-nolines");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({ lines: [] }),
      );
      expect(res.status).toBe(400);
      const body = await res.json();
      expect((body.errors?.[0] ?? "")).toContain("lines");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-numeric quantity is a 400 bad request", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-badqty");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({
          lines: [{ description: "X", quantity: "two", unitPriceExVat: 100 }],
        }),
      );
      expect(res.status).toBe(400);
      const body = await res.json();
      expect((body.errors?.[0] ?? "")).toContain("quantity");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing buyer is a 400 (same validator as issue)", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-nobuyer");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({ buyer: undefined }),
      );
      expect(res.status).toBe(400);
      const body = await res.json();
      expect(body.code).toBe("bad_request");
      expect((body.errors?.[0] ?? "")).toContain("buyer");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown customerId is a 409 conflict, not a 500", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-badcust");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody({ customerId: 9999 }),
      );
      expect(res.status).toBe(409);
      const body = await res.json();
      expect((body.errors?.[0] ?? "")).toContain("customer");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404", async () => {
    const { root: ws } = makeWorkspace("inv-preview-noco");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        "nope-aps",
        previewBody(),
      );
      expect(res.status).toBe(404);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a GET on the preview route is 405 method not allowed", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-method");
    try {
      const res = await handleRequest(
        new Request(
          `http://localhost/api/companies/${slug}/invoices/preview`,
          { method: "GET", headers: { host: "127.0.0.1" } },
        ),
        config({ workspaceRoot: ws }),
      );
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a preview from a non-loopback host is refused when auth is disabled", async () => {
    const { root: ws, slug } = makeWorkspace("inv-preview-remote");
    try {
      const res = await postPreview(
        config({ workspaceRoot: ws }),
        slug,
        previewBody(),
        { host: "example.com" },
      );
      expect(res.status).toBe(401);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
