// Follow-up regression guards from adversarial review of commit cdf2cbe.
// Covers sweep misses NOT caught by the prior pass:
//
//   src/server/router.ts:1031   "a company with that slug already exists"
//   src/server/router.ts:1063   "provide 'name' and/or 'archived' to update"
//   src/core/workspace.ts:217   "company slug 'X' is already registered ..."
//   src/core/workspace.ts:305   "company slug 'X' is already in the workspace manifest"
//   src/core/workspace.ts:311   "cannot adopt 'X': no ledger found ..."
//   src/server/write-handlers.ts: reminder/accountant fallback prose
//   src/server/errors.ts:96     "internal server error"
//   src/server/mutations.ts:128 body-size guard prose
//
// Each assertion fails BEFORE the fix.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  initWorkspace,
  registerWorkspaceCompany,
  adoptCompanyDir,
} from "../../src/core/workspace";
import { createCompany } from "../../src/core/company";
import { handleRequest } from "../../src/server/router";
import { toErrorResponse } from "../../src/server/errors";
import type { ServerConfig } from "../../src/server/config";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
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

async function json(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: (await res.json()) as { errors?: string[]; code?: string } };
}

describe("#242 follow-up — router.ts sweep misses", () => {
  test("POST /api/companies with a duplicate slug returns Danish conflict", async () => {
    const ws = makeWorkspace("dup-create", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        body: JSON.stringify({ name: "Acme ApS" }),
      });
      expect(res.status).toBe(409);
      expect(res.body.errors?.[0] ?? "").not.toMatch(/a company with that slug already exists/i);
      expect(res.body.errors?.[0] ?? "").toMatch(
        /der findes allerede en virksomhed med den slug/i,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("PATCH /api/companies/:slug with empty body returns Danish 400", async () => {
    const ws = makeWorkspace("patch-empty-prose", ["Acme ApS"]);
    try {
      const res = await json(config({ workspaceRoot: ws }), "/api/companies/acme-aps", {
        method: "PATCH",
        body: JSON.stringify({}),
      });
      expect(res.status).toBe(400);
      expect(res.body.errors?.[0] ?? "").not.toMatch(
        /provide 'name' and\/or 'archived' to update/i,
      );
      // Danish prose with field tokens kept English (per the schema-contract caveat).
      expect(res.body.errors?.[0] ?? "").toMatch(/angiv 'name' og\/eller 'archived'/i);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#242 follow-up — workspace.ts sweep misses", () => {
  test("registerWorkspaceCompany rejects a duplicate slug in Danish", () => {
    const ws = makeWorkspace("dup-register");
    try {
      registerWorkspaceCompany(ws, {
        slug: "acme",
        name: "Acme ApS",
        createdAt: "2026-05-26T00:00:00.000Z",
        archived: false,
      });
      expect(() =>
        registerWorkspaceCompany(ws, {
          slug: "acme",
          name: "Acme Duplicate",
          createdAt: "2026-05-26T00:00:00.000Z",
          archived: false,
        }),
      ).toThrow(/virksomheden med slug 'acme' er allerede registreret/i);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("adoptCompanyDir rejects a slug that is already in the manifest in Danish", () => {
    const ws = makeWorkspace("dup-adopt", ["Acme ApS"]);
    try {
      expect(() => adoptCompanyDir(ws, "acme-aps")).toThrow(
        /virksomheden med slug 'acme-aps' er allerede registreret/i,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("adoptCompanyDir rejects a missing ledger in Danish", () => {
    const ws = makeWorkspace("adopt-no-ledger");
    try {
      // Plant a directory shaped like a company slug but WITHOUT a ledger db.
      mkdirSync(join(ws, "ghost-aps"));
      expect(() => adoptCompanyDir(ws, "ghost-aps")).toThrow(
        /kan ikke adoptere 'ghost-aps': ingen ledger fundet/i,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("#242 follow-up — errors.ts generic 500 body", () => {
  test("toErrorResponse maps an unknown error to a Danish 500 body", () => {
    const { status, body } = toErrorResponse(new TypeError("internal stuff that must not leak"));
    expect(status).toBe(500);
    expect(body.code).toBe("internal");
    expect(body.errors[0]).not.toMatch(/internal server error/i);
    expect(body.errors[0]).toMatch(/intern serverfejl/i);
  });
});

describe("#242 follow-up — mutations.ts body-size guard prose", () => {
  test("PATCH with an oversized Content-Length is rejected in Danish", async () => {
    const ws = makeWorkspace("body-size");
    try {
      const huge = "a".repeat(2_000_000);
      const res = await json(config({ workspaceRoot: ws }), "/api/companies", {
        method: "POST",
        headers: { "content-length": String(huge.length) },
        body: JSON.stringify({ name: huge }),
      });
      expect([400, 413]).toContain(res.status);
      const msg = res.body.errors?.[0] ?? "";
      // Either the size guard fires (Danish prose) or some other 400 — but
      // crucially, the size-guard message must not be English when it does fire.
      if (/byte/i.test(msg) || /grænse/i.test(msg) || /limit/i.test(msg)) {
        expect(msg).not.toMatch(/request body exceeds the .* byte limit/i);
        expect(msg).toMatch(/request-body overskrider grænsen/i);
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

// Note: write-handlers fallback prose at lines 543/1987/2004/2024/2161 is
// surfaced only when the core layer returns ok:false with an empty errors[].
// In the production code path the core always populates errors when it
// returns ok:false, so these fallbacks are defensive — we test them at the
// source level by reading the file and asserting the literal Danish strings.
// (No HTTP fixture would deterministically trigger them without monkey-patching
// the imported core functions, which we deliberately avoid.)
describe("#242 follow-up — write-handlers.ts fallback prose is Danish", () => {
  test("the file no longer contains the English reminder/accountant fallback strings", async () => {
    const src = await Bun.file(
      `${import.meta.dir}/../../src/server/write-handlers.ts`,
    ).text();
    // None of these English fallback strings should remain in the file.
    expect(src).not.toContain("Reminder could not be registered.");
    expect(src).not.toContain("Reminder fee could not be booked.");
    expect(src).not.toContain("Reminder e-mail could not be sent.");
    // The schema-token-laden one keeps English field names but the prose framing
    // should be Danish.
    expect(src).not.toContain("provide at least one profile field to update");
    // "accountant export failed" is curated prose; should be Danish.
    expect(src).not.toContain('"accountant export failed"');
  });
});
