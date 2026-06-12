// Tests: src/server/router.ts, src/server/data/budget.ts,
// src/server/write-handlers.ts — the Budget cockpit endpoints (#339).
//
// Verifies the three new routes:
//   - GET  /api/companies/:slug/budget?year=         → latest-revision lines
//   - GET  /api/companies/:slug/budget-vs-actual?... → comparison report
//   - POST /api/companies/:slug/budget               → append a budget line
//
// The cockpit is a third caller of `src/core/budget.ts`, so the tests focus
// on the HTTP envelope, the route catalog entry, and the actor attribution —
// they do not re-test core's budget math (covered by tests/unit/budget.test.ts).
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest, ROUTE_CATALOG } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";
import { createCompany } from "../../src/core/company";
import {
  initWorkspace,
  companyRootForSlug,
} from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import { setBudget } from "../../src/core/budget";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function makeWorkspace(label: string) {
  const root = tmpRoot(label);
  initWorkspace(root);
  createCompany(root, { name: "Acme ApS" });
  return root;
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

async function call(
  cfg: ServerConfig,
  method: string,
  path: string,
  body?: unknown,
) {
  // The localhost hard-gate (`withCompanyMutation`) inspects the `host` header;
  // every cockpit write test in this repo sends `host: "127.0.0.1"` explicitly
  // so the in-process Request mirrors a real loopback HTTP request.
  const headers: Record<string, string> = { host: "127.0.0.1" };
  const init: RequestInit = { method, headers };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
    headers["content-type"] = "application/json";
  }
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

/**
 * Opens the company's ledger and returns the first expense/income/asset
 * account number — same trick `tests/unit/budget.test.ts` uses to stay chart
 * agnostic, the cockpit data layer must reflect whatever migrate() seeds.
 */
function expenseAccountNo(ws: string, slug: string): string {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    return (db
      .query(
        "SELECT account_no FROM accounts WHERE type = 'expense' ORDER BY account_no LIMIT 1",
      )
      .get() as { account_no: string }).account_no;
  } finally {
    db.close();
  }
}

function bankAccountNo(ws: string, slug: string): string {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    return (db
      .query(
        "SELECT account_no FROM accounts WHERE type = 'asset' ORDER BY account_no LIMIT 1",
      )
      .get() as { account_no: string }).account_no;
  } finally {
    db.close();
  }
}

/** Sets a budget line directly via core (no HTTP) so a read test can seed. */
function seedBudget(ws: string, slug: string, accountNo: string, period: string, amount: number) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const r = setBudget(db, { accountNo, period, amount });
    if (!r.ok) throw new Error("seed setBudget failed: " + r.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Inserts a balanced expense row directly via SQL so the budget-vs-actual
 * report sees real ledger movement. Goes around `postJournalEntry`'s
 * document requirement (expense lines normally need a bilag), mirroring the
 * approach `tests/unit/budget.test.ts` uses for the same reason.
 */
function postExpense(
  ws: string,
  slug: string,
  date: string,
  expenseAcc: string,
  bankAcc: string,
  amount: number,
) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const accId = (no: string) =>
      (db.query("SELECT id FROM accounts WHERE account_no = ?").get(no) as {
        id: number;
      }).id;
    const entry = db
      .query(
        `INSERT INTO journal_entries (entry_no, transaction_date, text, rule_version, entry_hash)
         VALUES (?, ?, ?, 'test', 'h') RETURNING id`,
      )
      .get(`E-${date}-${expenseAcc}-${amount}`, date, "test expense") as {
      id: number;
    };
    db.run(
      "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, ?, 0)",
      entry.id,
      accId(expenseAcc),
      amount,
    );
    db.run(
      "INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount) VALUES (?, ?, 0, ?)",
      entry.id,
      accId(bankAcc),
      amount,
    );
  } finally {
    db.close();
  }
}

describe("cockpit budget API (#339) — route catalog", () => {
  test("the catalog advertises the three budget routes", () => {
    const patterns = ROUTE_CATALOG.map((r) => `${r.method} ${r.pattern}`);
    expect(patterns).toContain("GET /api/companies/:slug/budget");
    expect(patterns).toContain("GET /api/companies/:slug/budget-vs-actual");
    expect(patterns).toContain("POST /api/companies/:slug/budget");
  });
});

describe("GET /api/companies/:slug/budget", () => {
  test("returns the company's effective budget lines for the year", async () => {
    const ws = makeWorkspace("budget-get");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      seedBudget(ws, "acme-aps", acc, "2026-06", 5000);
      seedBudget(ws, "acme-aps", acc, "2026-07", 7000);
      // A second revision on June must collapse to the latest amount.
      seedBudget(ws, "acme-aps", acc, "2026-06", 5500);

      const res = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      const b = res.body.budget;
      expect(b.slug).toBe("acme-aps");
      expect(b.selectedYear).toBe("2026");
      expect(b.periodStart).toBe("2026-01");
      expect(b.periodEnd).toBe("2026-12");
      // 12 periods skeleton, regardless of how many lines are filled.
      expect(b.periods).toHaveLength(12);
      expect(b.periods[0]).toBe("2026-01");
      expect(b.periods[11]).toBe("2026-12");
      // Two effective lines: 5500 (latest June) + 7000 (July) = 12500.
      expect(b.lines.length).toBe(2);
      const june = b.lines.find(
        (l: { accountNo: string; period: string }) =>
          l.accountNo === acc && l.period === "2026-06",
      );
      expect(june.amount).toBe(5500);
      expect(b.totalBudget).toBe(12500);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("budget-get-404");
    try {
      const res = await call(
        config(ws),
        "GET",
        "/api/companies/ghost/budget?year=2026",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("rejects a malformed year query value", async () => {
    const ws = makeWorkspace("budget-get-bady");
    try {
      const res = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget?year=99",
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("GET /api/companies/:slug/budget-vs-actual", () => {
  test("returns the comparison report with deterministic figures", async () => {
    const ws = makeWorkspace("bva-ok");
    try {
      const expense = expenseAccountNo(ws, "acme-aps");
      const bank = bankAccountNo(ws, "acme-aps");
      seedBudget(ws, "acme-aps", expense, "2026-06", 5000);
      // Two expense postings: 3000 + 1000 = 4000.
      postExpense(ws, "acme-aps", "2026-06-05", expense, bank, 3000);
      postExpense(ws, "acme-aps", "2026-06-20", expense, bank, 1000);

      const res = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget-vs-actual?year=2026",
      );
      expect(res.status).toBe(200);
      const r = res.body.budgetVsActual;
      expect(r.slug).toBe("acme-aps");
      expect(r.selectedYear).toBe("2026");
      expect(r.periodStart).toBe("2026-01");
      expect(r.periodEnd).toBe("2026-12");
      const line = r.lines.find(
        (l: { accountNo: string; period: string }) =>
          l.accountNo === expense && l.period === "2026-06",
      );
      expect(line.budget).toBe(5000);
      expect(line.actual).toBe(4000);
      // For an expense, variance = budget - actual; positive => under budget.
      expect(line.variance).toBe(1000);
      // variancePercent = 1000 / 5000 = 0.2.
      expect(line.variancePercent).toBeCloseTo(0.2, 6);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("variancePercent is null when budget is zero (no division by zero)", async () => {
    const ws = makeWorkspace("bva-zero");
    try {
      const expense = expenseAccountNo(ws, "acme-aps");
      const bank = bankAccountNo(ws, "acme-aps");
      // No budget set; only an actual posting — budget side is zero.
      postExpense(ws, "acme-aps", "2026-06-10", expense, bank, 1234);
      const res = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget-vs-actual?year=2026",
      );
      expect(res.status).toBe(200);
      const line = res.body.budgetVsActual.lines.find(
        (l: { accountNo: string; budget: number }) =>
          l.accountNo === expense && l.budget === 0,
      );
      expect(line.variancePercent).toBeNull();
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});

describe("POST /api/companies/:slug/budget", () => {
  test("appends a budget line and returns its id", async () => {
    const ws = makeWorkspace("budget-post");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: acc,
        period: "2026-06",
        amount: 5000,
        notes: "Plan for sommeren",
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.budget.accountNo).toBe(acc);
      expect(res.body.budget.period).toBe("2026-06");
      expect(res.body.budget.amount).toBe(5000);
      expect(typeof res.body.budget.id).toBe("number");

      // The line is now visible to a follow-up GET.
      const list = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget?year=2026",
      );
      expect(list.body.budget.lines.length).toBe(1);
      expect(list.body.budget.lines[0].notes).toBe("Plan for sommeren");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("two POSTs for the same (account, period) keep the latest revision", async () => {
    const ws = makeWorkspace("budget-revisions");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: acc,
        period: "2026-06",
        amount: 5000,
      });
      const second = await call(
        config(ws),
        "POST",
        "/api/companies/acme-aps/budget",
        { accountNo: acc, period: "2026-06", amount: 5500 },
      );
      expect(second.status).toBe(200);
      const list = await call(
        config(ws),
        "GET",
        "/api/companies/acme-aps/budget?year=2026",
      );
      // The effective line is the latest revision, not both.
      expect(list.body.budget.lines.length).toBe(1);
      expect(list.body.budget.lines[0].amount).toBe(5500);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown account is a 409 conflict (does-not-exist heuristic)", async () => {
    const ws = makeWorkspace("budget-bad-acc");
    try {
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: "999999",
        period: "2026-06",
        amount: 1000,
      });
      // The shared withCompanyMutation pipeline maps "does not exist" → 409 by
      // its conflict heuristic, since the target is genuinely missing rather
      // than the input being malformed. The core's message is surfaced.
      expect(res.status).toBe(409);
      expect(res.body.ok).toBe(false);
      expect(res.body.code).toBe("conflict");
      expect((res.body.errors?.[0] ?? "")).toContain("999999");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a missing accountNo body field is a 400", async () => {
    const ws = makeWorkspace("budget-missing-acc");
    try {
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        period: "2026-06",
        amount: 1000,
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a malformed period body field is a 400", async () => {
    const ws = makeWorkspace("budget-bad-period");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: acc,
        period: "2026-13",
        amount: 1000,
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a negative amount is a 400", async () => {
    const ws = makeWorkspace("budget-neg");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: acc,
        period: "2026-06",
        amount: -100,
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a non-numeric amount is a 400", async () => {
    const ws = makeWorkspace("budget-nan");
    try {
      const acc = expenseAccountNo(ws, "acme-aps");
      const res = await call(config(ws), "POST", "/api/companies/acme-aps/budget", {
        accountNo: acc,
        period: "2026-06",
        amount: "fem-tusind",
      });
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a wrong method on /budget is 405", async () => {
    const ws = makeWorkspace("budget-405");
    try {
      const res = await call(config(ws), "DELETE", "/api/companies/acme-aps/budget");
      expect(res.status).toBe(405);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
