// Tests: src/server/router.ts, src/server/data.ts, src/server/write-handlers.ts
// — the Cockpit company-profile/bank editing endpoint (#284), the close-period
// endpoint (#287) and the annual-report deadline in the Obligations payload
// (#290).
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string, companyName = "Acme ApS") {
  const root = tmpRoot(label);
  initWorkspace(root);
  const created = createCompany(root, { name: companyName });
  return { root, slug: created.slug };
}

function config(workspaceRoot: string): ServerConfig {
  return { host: "127.0.0.1", port: 0, authRequired: false, authToken: null, workspaceRoot };
}

async function call(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

async function patch(cfg: ServerConfig, path: string, body: unknown) {
  return call(cfg, path, {
    method: "PATCH",
    headers: { host: "127.0.0.1" },
    body: JSON.stringify(body),
  });
}

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = { method: "POST", headers: { host: "127.0.0.1" } };
  if (body !== undefined) init.body = JSON.stringify(body);
  return call(cfg, path, init);
}

/** Posts a P&L entry so a company has bookable activity in a quarter. */
function postPnlEntry(ws: string, slug: string, transactionDate: string) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const res = postJournalEntry(db, {
      transactionDate,
      text: "Test posting",
      lines: [
        { accountNo: "1100", debitAmount: 100 },
        { accountNo: "2000", creditAmount: 100 },
      ],
    });
    if (!res.ok) throw new Error(res.errors.join("; "));
  } finally {
    db.close();
  }
}

// --------------------------------------------------------------------------
// #284 — Cockpit owner can set company profile + bank/payment details.
// --------------------------------------------------------------------------
describe("Cockpit company profile / bank details (#284)", () => {
  test("GET .../company surfaces the payment block (null when unset)", async () => {
    const { root: ws, slug } = makeWorkspace("profile-get");
    try {
      const { status, body } = await call(config(ws), `/api/companies/${slug}/company`);
      expect(status).toBe(200);
      expect(body.ok).toBe(true);
      // A freshly-created company has no bank account, so payment is null.
      expect(body.company).toHaveProperty("payment");
      expect(body.company.payment).toBeNull();
    } finally {
      cleanupDir(ws);
    }
  });

  test("PATCH .../company saves bank/payment details and they round-trip", async () => {
    const { root: ws, slug } = makeWorkspace("profile-bank");
    try {
      const res = await patch(config(ws), `/api/companies/${slug}/company`, {
        address: "Vej 1",
        postalCode: "1000",
        city: "København",
        paymentTermsDays: 30,
        payment: {
          bankName: "Danske Bank",
          registrationNo: "1234",
          accountNo: "0001234567",
        },
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.company.address).toBe("Vej 1");
      expect(res.body.company.payment.bankName).toBe("Danske Bank");
      expect(res.body.company.payment.accountNo).toBe("0001234567");

      // The persisted state is what a fresh GET returns.
      const fresh = await call(config(ws), `/api/companies/${slug}/company`);
      expect(fresh.body.company.payment.bankName).toBe("Danske Bank");
      expect(fresh.body.company.payment.registrationNo).toBe("1234");
      expect(fresh.body.company.address).toBe("Vej 1");
    } finally {
      cleanupDir(ws);
    }
  });

  test("PATCH .../company with no recognised fields is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("profile-empty");
    try {
      const res = await patch(config(ws), `/api/companies/${slug}/company`, {});
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("PATCH .../company for an unknown slug is a safe 404", async () => {
    const root = tmpRoot("profile-404");
    initWorkspace(root);
    try {
      const res = await patch(config(root), "/api/companies/ghost/company", {
        address: "Vej 1",
      });
      expect(res.status).toBe(404);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(root);
    }
  });

  test("PATCH .../company rejects an invalid CVR with a 400", async () => {
    const { root: ws, slug } = makeWorkspace("profile-bad-cvr");
    try {
      const res = await patch(config(ws), `/api/companies/${slug}/company`, {
        cvr: "12",
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #287 — Cockpit can close an accounting period.
// --------------------------------------------------------------------------
describe("Cockpit close period (#287)", () => {
  test("POST .../periods/close closes a vat_quarter period", async () => {
    const { root: ws, slug } = makeWorkspace("close-period");
    try {
      postPnlEntry(ws, slug, "2026-02-15");
      const res = await post(config(ws), `/api/companies/${slug}/periods/close`, {
        periodStart: "2026-01-01",
        periodEnd: "2026-03-31",
        confirm: true,
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.period.periodStart).toBe("2026-01-01");
      expect(res.body.period.periodEnd).toBe("2026-03-31");
      expect(res.body.period.status).toBe("closed");

      // The period row exists and is closed.
      const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
      try {
        migrate(db);
        const row = db
          .query(
            "SELECT status FROM accounting_periods WHERE period_start = ? AND period_end = ?",
          )
          .get("2026-01-01", "2026-03-31") as { status: string } | null;
        expect(row?.status).toBe("closed");
      } finally {
        db.close();
      }
    } finally {
      cleanupDir(ws);
    }
  });

  test("POST .../periods/close without confirm is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("close-noconfirm");
    try {
      const res = await post(config(ws), `/api/companies/${slug}/periods/close`, {
        periodStart: "2026-01-01",
        periodEnd: "2026-03-31",
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("closing the same period twice is a 409 conflict", async () => {
    const { root: ws, slug } = makeWorkspace("close-twice");
    try {
      const body = {
        periodStart: "2026-01-01",
        periodEnd: "2026-03-31",
        confirm: true,
      };
      const first = await post(config(ws), `/api/companies/${slug}/periods/close`, body);
      expect(first.status).toBe(200);
      const second = await post(config(ws), `/api/companies/${slug}/periods/close`, body);
      expect(second.status).toBe(409);
    } finally {
      cleanupDir(ws);
    }
  });

  test("a GET on the close-period route is 405 — it is POST-only", async () => {
    const { root: ws, slug } = makeWorkspace("close-405");
    try {
      const res = await call(config(ws), `/api/companies/${slug}/periods/close`);
      expect(res.status).toBe(405);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #290 — the annual-report (årsrapport) filing deadline is in Obligations.
// --------------------------------------------------------------------------
describe("Cockpit obligations — annual report deadline (#290)", () => {
  test("the obligations list includes the årsrapport filing deadline", async () => {
    const { root: ws, slug } = makeWorkspace("oblig-annual");
    try {
      postPnlEntry(ws, slug, "2026-02-15");
      const res = await call(
        config(ws),
        `/api/companies/${slug}/obligations?year=2026`,
      );
      expect(res.status).toBe(200);
      const rows = res.body.obligations.obligations as Array<{
        kind: string;
        label: string;
        dueDate: string | null;
      }>;
      const annual = rows.find((r) => r.kind === "annual-report");
      expect(annual).toBeDefined();
      // FY 2026 ends 2026-12-31; the årsrapport is due 1 May 2027 (the 1st
      // of the 5th month after the fiscal year ends) — the SAME deadline
      // `agent run` (src/agent/loop.ts#checkDeadlines) computes.
      expect(annual!.dueDate).toBe("2027-05-01");
      expect(annual!.label).toMatch(/[ÅA]rsrapport/);
    } finally {
      cleanupDir(ws);
    }
  });
});
