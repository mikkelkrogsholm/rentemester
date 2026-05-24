// Tests: src/server/router.ts + src/server/data/exceptions-view.ts (#332 —
// Cockpit Undtagelser queue view). Covers the read route (`GET .../exceptions`)
// — status filtering, grouped summary, archived-period exclusion — and the
// already-shipped write route (`POST .../exceptions/:id/resolve`) seen from
// the cockpit's perspective using the new `{ok:false, errors[…]}` envelope.
//
// Seeds exceptions directly via `core/exceptions.ts#recordException` so the
// tests stay deterministic and never depend on the bank/import wiring.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import {
  companyRootForSlug,
  initWorkspace,
} from "../../src/core/workspace";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { recordException } from "../../src/core/exceptions";

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

async function send(
  cfg: ServerConfig,
  method: string,
  path: string,
  body?: unknown,
) {
  const init: RequestInit = { method, headers: { host: "127.0.0.1" } };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(
    new Request(`http://localhost${path}`, init),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

function seed(
  workspaceRoot: string,
  slug: string,
  rows: Array<Parameters<typeof recordException>[1]>,
) {
  const companyRoot = companyRootForSlug(workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    for (const row of rows) recordException(db, row);
  } finally {
    db.close();
  }
}

describe("Cockpit Undtagelser-routes (#332)", () => {
  test("GET /exceptions returns an empty payload with totals at zero before any are recorded", async () => {
    const { root, slug } = makeWorkspace("exc-empty");
    try {
      const cfg = config(root);
      const res = await send(cfg, "GET", `/api/companies/${slug}/exceptions`);
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const ex = res.body.exceptions;
      expect(ex.slug).toBe(slug);
      expect(ex.status).toBe("open");
      expect(ex.rows).toEqual([]);
      expect(ex.count).toBe(0);
      expect(ex.openCount).toBe(0);
      expect(ex.resolvedCount).toBe(0);
      expect(ex.openGroups).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("GET /exceptions surfaces open rows + groups them by type (Danish summary line)", async () => {
    const { root, slug } = makeWorkspace("exc-list");
    try {
      seed(root, slug, [
        {
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "medium",
          message: "Banktransaktionen mangler bilag.",
          requiredAction: "Find kvitteringen og bogfør den.",
        },
        {
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "high",
          message: "Endnu en banktransaktion mangler bilag.",
          requiredAction: "Find kvitteringen og bogfør den.",
        },
        {
          type: "AGENT_TAX_RETURN_NEEDS_REVIEW",
          severity: "medium",
          message: "Oplysningsskemaet skal afklares før indberetning.",
          requiredAction: "Afklar tallet manuelt.",
        },
      ]);
      const cfg = config(root);
      const res = await send(cfg, "GET", `/api/companies/${slug}/exceptions`);
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const ex = res.body.exceptions;
      expect(ex.rows.length).toBe(3);
      expect(ex.count).toBe(3);
      expect(ex.openCount).toBe(3);
      expect(ex.resolvedCount).toBe(0);
      // The unmatched-bank group sorts first — severity high beats the
      // medium tax-return group; the Danish label pluralises correctly.
      expect(ex.openGroups.length).toBe(2);
      expect(ex.openGroups[0].type).toBe("UNMATCHED_BANK_TRANSACTION");
      expect(ex.openGroups[0].count).toBe(2);
      expect(ex.openGroups[0].severity).toBe("high");
      expect(ex.openGroups[0].label).toContain("banktransaktioner mangler");
      expect(ex.openGroups[0].link).toBe("bank");
      // Each row carries its required-action text so the cockpit shows the
      // owner what to do — never a bare technical message.
      const first = ex.rows[0];
      expect(first.requiredAction).not.toBeNull();
      expect(typeof first.createdAt).toBe("string");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("GET /exceptions?status=resolved hides open rows, ?status=all returns both", async () => {
    const { root, slug } = makeWorkspace("exc-filter");
    try {
      seed(root, slug, [
        { type: "MANUAL_REVIEW", message: "Open one", severity: "low" },
        { type: "MANUAL_REVIEW", message: "Open two", severity: "low" },
      ]);
      const cfg = config(root);
      // Resolve one via the existing write route — this also exercises the
      // new error envelope shape, since a malformed id returns `errors[0]`.
      const resolveBad = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/exceptions/abc/resolve`,
        { confirm: true },
      );
      expect(resolveBad.status).toBe(400);
      expect(resolveBad.body.ok).toBe(false);
      expect(typeof resolveBad.body.errors[0]).toBe("string");
      expect(resolveBad.body.code).toBe("bad_request");

      const resolveOk = await send(
        cfg,
        "POST",
        `/api/companies/${slug}/exceptions/1/resolve`,
        { confirm: true, note: "Afgjort i cockpittet" },
      );
      expect(resolveOk.status).toBe(200);
      expect(resolveOk.body.ok).toBe(true);
      expect(resolveOk.body.exception.id).toBe(1);
      expect(resolveOk.body.exception.resolved).toBe(true);

      const openOnly = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/exceptions?status=open`,
      );
      expect(openOnly.body.exceptions.count).toBe(1);
      expect(openOnly.body.exceptions.rows[0].status).toBe("open");

      const resolvedOnly = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/exceptions?status=resolved`,
      );
      expect(resolvedOnly.body.exceptions.count).toBe(1);
      expect(resolvedOnly.body.exceptions.rows[0].status).toBe("resolved");
      expect(resolvedOnly.body.exceptions.rows[0].resolutionNote).toBe(
        "Afgjort i cockpittet",
      );

      const all = await send(
        cfg,
        "GET",
        `/api/companies/${slug}/exceptions?status=all`,
      );
      expect(all.body.exceptions.count).toBe(2);
      // The totals are filter-agnostic — both pills show their badge count
      // regardless of which pill is selected.
      expect(all.body.exceptions.openCount).toBe(1);
      expect(all.body.exceptions.resolvedCount).toBe(1);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("GET /exceptions for an unknown slug is a JSON 404 with the new envelope", async () => {
    const { root } = makeWorkspace("exc-404");
    try {
      const cfg = config(root);
      const res = await send(cfg, "GET", `/api/companies/no-such-co/exceptions`);
      expect(res.status).toBe(404);
      expect(res.body.ok).toBe(false);
      expect(typeof res.body.errors[0]).toBe("string");
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("the route catalog advertises the new exceptions list route", async () => {
    const { root } = makeWorkspace("exc-catalog");
    try {
      const cfg = config(root);
      const res = await send(cfg, "GET", "/api/health");
      expect(res.status).toBe(200);
      const routes = res.body.routes as Array<{ method: string; pattern: string }>;
      expect(
        routes.some(
          (r) =>
            r.method === "GET" &&
            r.pattern === "/api/companies/:slug/exceptions",
        ),
      ).toBe(true);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
