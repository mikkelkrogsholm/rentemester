// Tests: the agent-suggestions read route, plus the approve/reject write
// routes (#346).
//
// The cockpit's Agent-forslag view drives off `/api/companies/:slug/agent-
// suggestions`. The view lists open `AGENT_*` exceptions, and the owner
// approves or rejects each one. Approving/rejecting resolves the underlying
// exception with a decision-flavoured Danish note — it never posts a ledger
// entry on its own (a separate action-specific write route does the posting,
// e.g. "Beregn afskrivning" on the Anlæg view).

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

function tmpRoot(label: string): string {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string): { root: string; slug: string } {
  const root = tmpRoot(label);
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

/**
 * Seeds one open `AGENT_*` exception of `type` with a stable Danish rationale
 * and a `source_evidence.rule` id, then returns its id. Used by every test
 * below; the agent-suggestions read view looks for exactly this shape.
 */
function seedAgentException(
  ws: string,
  slug: string,
  options: {
    type: string;
    severity?: "low" | "medium" | "high";
    message: string;
    requiredAction?: string;
    rule?: string;
  },
): number {
  return withLedger(ws, slug, (db) => {
    const res = recordException(db, {
      type: options.type,
      severity: options.severity ?? "medium",
      message: options.message,
      requiredAction: options.requiredAction ?? null,
      sourceEvidence: { rule: options.rule ?? "DK-AGENT-001", payableId: 1 },
      postingPreview: { bankTransactionId: 1 },
    });
    if (!res.ok || !res.exceptionId) {
      throw new Error(`failed to seed exception: ${res.errors.join("; ")}`);
    }
    return res.exceptionId;
  });
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

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = {
    method: "POST",
    headers: { host: "127.0.0.1", "content-type": "application/json" },
  };
  if (body !== undefined) init.body = JSON.stringify(body);
  const res = await handleRequest(
    new Request(`http://localhost${path}`, init),
    cfg,
  );
  return { status: res.status, body: await res.json() };
}

// ---------------------------------------------------------------------------
// GET /api/companies/:slug/agent-suggestions
// ---------------------------------------------------------------------------

describe("Agent-forslag — read (#346)", () => {
  test("lists every open AGENT_* exception with rule, rationale and severity", async () => {
    const { root: ws, slug } = makeWorkspace("agent-list");
    try {
      const overdueId = seedAgentException(ws, slug, {
        type: "AGENT_PAYABLE_OVERDUE",
        severity: "high",
        message:
          "kreditorpost RE-123 til Acme A/S på 1.000,00 kr. med forfald 2026-05-01 er overforfalden og endnu ikke betalt.",
        requiredAction:
          "Betal kreditorposten, og afstem den udgående bankbetaling mod den med 'payable pay'.",
        rule: "DK-PAYABLE-001",
      });
      seedAgentException(ws, slug, {
        type: "AGENT_ACCRUAL_RECOGNITION_DUE",
        severity: "medium",
        message:
          'Periodeafgrænsningspost "Forsikring" — periode 4/12 (1.000,00 kr.) med planlagt bogføringsdato 2026-05-01 er forfalden og endnu ikke bogført.',
        rule: "DK-BOOKKEEPING-ACCRUAL-001",
      });

      const res = await get(config(ws), `/api/companies/${slug}/agent-suggestions`);
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const data = res.body.agentSuggestions;
      expect(data.count).toBe(2);
      expect(data.bySeverity.high).toBe(1);
      expect(data.bySeverity.medium).toBe(1);
      // Severity DESC → the high-severity payable row is first.
      const first = data.rows[0];
      expect(first.exceptionId).toBe(overdueId);
      expect(first.type).toBe("AGENT_PAYABLE_OVERDUE");
      expect(first.severity).toBe("high");
      expect(first.ruleId).toBe("DK-PAYABLE-001");
      expect(first.kindLabel).toContain("kreditorpost");
      expect(first.link).toBe("leverandoerfaktura");
      expect(first.rationale).toContain("er overforfalden");
      expect(first.requiredAction).toContain("Betal kreditorposten");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ignores non-AGENT_ exceptions (e.g. UNMATCHED_BANK_TRANSACTION)", async () => {
    const { root: ws, slug } = makeWorkspace("agent-filter");
    try {
      withLedger(ws, slug, (db) => {
        recordException(db, {
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "medium",
          message: "Banktransaktion mangler afstemning",
        });
      });
      seedAgentException(ws, slug, {
        type: "AGENT_POSSIBLE_FIXED_ASSET",
        message: "Bilag over 14.300 kr. — kan være et anlæg.",
      });

      const res = await get(config(ws), `/api/companies/${slug}/agent-suggestions`);
      expect(res.status).toBe(200);
      expect(res.body.agentSuggestions.count).toBe(1);
      expect(res.body.agentSuggestions.rows[0].type).toBe(
        "AGENT_POSSIBLE_FIXED_ASSET",
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("ignores resolved AGENT_* exceptions (only open queue shows)", async () => {
    const { root: ws, slug } = makeWorkspace("agent-resolved");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_PAYABLE_OVERDUE",
        message: "Overforfalden kreditorpost.",
      });
      // Approve, then list — the resolved row must NOT appear.
      const approve = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
      );
      expect(approve.status).toBe(200);

      const res = await get(config(ws), `/api/companies/${slug}/agent-suggestions`);
      expect(res.body.agentSuggestions.count).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty ledger returns count:0 with the unified envelope", async () => {
    const { root: ws, slug } = makeWorkspace("agent-empty");
    try {
      const res = await get(config(ws), `/api/companies/${slug}/agent-suggestions`);
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.agentSuggestions.count).toBe(0);
      expect(res.body.agentSuggestions.rows).toEqual([]);
      expect(res.body.agentSuggestions.bySeverity).toEqual({
        high: 0,
        medium: 0,
        low: 0,
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown company slug is a 404 with the unified envelope shape", async () => {
    const { root: ws } = makeWorkspace("agent-noco");
    try {
      const res = await get(config(ws), `/api/companies/nope-aps/agent-suggestions`);
      expect(res.status).toBe(404);
      expect(res.body.ok).toBe(false);
      expect(res.body.code).toBe("not_found");
      expect(Array.isArray(res.body.errors)).toBe(true);
      expect(res.body.errors.length).toBeGreaterThan(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// POST /api/companies/:slug/agent-suggestions/:id/approve
// ---------------------------------------------------------------------------

describe("Agent-forslag — godkend (#346)", () => {
  test("approving an open suggestion resolves it with a 'Godkendt'-note", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-ok");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_PAYABLE_OVERDUE",
        message: "Overforfalden kreditorpost.",
      });
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
        { note: "betalt manuelt" },
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.suggestion).toEqual({
        id,
        decision: "approved",
        resolved: true,
      });

      withLedger(ws, slug, (db) => {
        const row = db
          .query(
            "SELECT status, resolution_note, resolved_by FROM exceptions WHERE id = ?",
          )
          .get(id) as {
          status: string;
          resolution_note: string | null;
          resolved_by: string | null;
        };
        expect(row.status).toBe("resolved");
        expect(row.resolution_note).toContain("Godkendt af ejer i cockpit");
        expect(row.resolution_note).toContain("betalt manuelt");
        // The cockpit-actor is the canonical attributor.
        expect(row.resolved_by).toBe("system:cockpit");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("approval works without a note — the audit row still says 'Godkendt'", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-nonote");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_ACCRUAL_RECOGNITION_DUE",
        message: "Periodeafgrænsning klar.",
      });
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
      );
      expect(res.status).toBe(200);
      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT resolution_note FROM exceptions WHERE id = ?")
          .get(id) as { resolution_note: string };
        expect(row.resolution_note).toBe("Godkendt af ejer i cockpit");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("approving twice is a 409 conflict — the second click is a no-op", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-twice");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_PAYABLE_OVERDUE",
        message: "Overforfalden kreditorpost.",
      });
      const cfg = config(ws);
      const first = await post(
        cfg,
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
      );
      expect(first.status).toBe(200);
      const second = await post(
        cfg,
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
      );
      expect(second.status).toBe(409);
      expect(second.body.ok).toBe(false);
      expect(second.body.code).toBe("conflict");
      expect(second.body.errors[0]).toContain("allerede");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("approving a non-AGENT_ exception is a 400 — wrong row type", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-wrongtype");
    try {
      const id = withLedger(ws, slug, (db) => {
        const r = recordException(db, {
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "medium",
          message: "Banktransaktion mangler afstemning.",
        });
        return r.exceptionId!;
      });
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/${id}/approve`,
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
      expect(res.body.errors[0]).toContain("ikke et agent-forslag");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("approving an unknown id is a 409 with the unified envelope", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-missing");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/9999/approve`,
      );
      expect(res.status).toBe(409);
      expect(res.body.ok).toBe(false);
      expect(res.body.code).toBe("conflict");
      expect(res.body.errors[0]).toContain("findes ikke");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-integer id segment is a 400 bad_request", async () => {
    const { root: ws, slug } = makeWorkspace("agent-approve-badid");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/abc/approve`,
      );
      expect(res.status).toBe(404);
      // /abc/ doesn't match the dispatch regex (\d+), so it falls through to
      // the "ukendt endpoint" 404 — that's the friendlier outcome and is also
      // consistent with how every other id-bearing route in this router
      // handles a non-numeric segment.
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// ---------------------------------------------------------------------------
// POST /api/companies/:slug/agent-suggestions/:id/reject
// ---------------------------------------------------------------------------

describe("Agent-forslag — afvis (#346)", () => {
  test("rejecting an open suggestion resolves it with an 'Afvist'-note + the owner reason", async () => {
    const { root: ws, slug } = makeWorkspace("agent-reject-ok");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_POSSIBLE_FIXED_ASSET",
        message: "Bilag over 14.300 kr. — kan være et anlæg.",
      });
      const res = await post(
        config(ws),
        `/api/companies/${slug}/agent-suggestions/${id}/reject`,
        { note: "ikke et anlæg — det er forbrugsmateriale" },
      );
      expect(res.status).toBe(200);
      expect(res.body.suggestion).toEqual({
        id,
        decision: "rejected",
        resolved: true,
      });

      withLedger(ws, slug, (db) => {
        const row = db
          .query("SELECT status, resolution_note FROM exceptions WHERE id = ?")
          .get(id) as { status: string; resolution_note: string };
        expect(row.status).toBe("resolved");
        expect(row.resolution_note).toContain("Afvist af ejer i cockpit");
        expect(row.resolution_note).toContain("forbrugsmateriale");
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rejecting twice is a 409 conflict — the second click is a no-op", async () => {
    const { root: ws, slug } = makeWorkspace("agent-reject-twice");
    try {
      const id = seedAgentException(ws, slug, {
        type: "AGENT_PAYABLE_OVERDUE",
        message: "Overforfalden kreditorpost.",
      });
      const cfg = config(ws);
      const first = await post(
        cfg,
        `/api/companies/${slug}/agent-suggestions/${id}/reject`,
      );
      expect(first.status).toBe(200);
      const second = await post(
        cfg,
        `/api/companies/${slug}/agent-suggestions/${id}/reject`,
      );
      expect(second.status).toBe(409);
      expect(second.body.code).toBe("conflict");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
