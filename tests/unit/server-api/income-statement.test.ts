import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — income statement (GET .../income-statement)", () => {
  test("returns grouped income/expense lines and the result for the year", async () => {
    const ws = makeWorkspace("is-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2026",
      );
      expect(res.status).toBe(200);
      const is = res.body.incomeStatement;
      expect(is.slug).toBe("acme-aps");
      expect(is.selectedYear).toBe("2026");
      expect(is.archived).toBe(false);
      expect(is.totalIncome).toBe(1000);
      expect(is.totalExpense).toBe(400);
      expect(is.result).toBe(600);
      expect(is.income[0]).toMatchObject({ amount: 1000, priorAmount: 0 });
      expect(is.expense[0]).toMatchObject({ amount: 400, priorAmount: 0 });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a prior-year posting surfaces as the comparison amount", async () => {
    const ws = makeWorkspace("is-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2025-04-01", 800, 0);
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 0);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.incomeStatement.income[0].amount).toBe(1000);
      expect(res.body.incomeStatement.income[0].priorAmount).toBe(800);
      expect(res.body.incomeStatement.priorTotalIncome).toBe(800);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("income-statement for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("is-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/income-statement",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
