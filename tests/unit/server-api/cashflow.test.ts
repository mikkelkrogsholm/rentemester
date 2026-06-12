import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, rmSync, seedBankTransaction } from "./_shared";

describe("cockpit API — cash flow (GET .../cashflow)", () => {
  test("computes monthly in/out and the balance trajectory from bank rows", async () => {
    const ws = makeWorkspace("cf-live", ["Acme ApS"]);
    try {
      seedBankTransaction(ws, "acme-aps", "2026-02-10", "Indbetaling", 1000, 1000);
      seedBankTransaction(ws, "acme-aps", "2026-02-20", "Gebyr", -200, 800);
      seedBankTransaction(ws, "acme-aps", "2026-05-05", "Indbetaling", 500, 1300);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.slug).toBe("acme-aps");
      expect(cf.archived).toBe(false);
      expect(cf.hasTransactions).toBe(true);
      expect(cf.months.length).toBe(12);
      // February: 1000 in, 200 out, 800 net.
      const feb = cf.months[1];
      expect(feb.indbetalinger).toBe(1000);
      expect(feb.udbetalinger).toBe(200);
      expect(feb.netto).toBe(800);
      // May: 500 in only.
      expect(cf.months[4].indbetalinger).toBe(500);
      // Year totals + closing balance from the latest balance_after.
      expect(cf.totalIn).toBe(1500);
      expect(cf.totalOut).toBe(200);
      expect(cf.closingBalance).toBe(1300);
      expect(cf.balanceSeries.length).toBe(3);
      expect(cf.balanceSeries[cf.balanceSeries.length - 1].balance).toBe(1300);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("opening balance is the actual balance before the year starts", async () => {
    const ws = makeWorkspace("cf-opening", ["Acme ApS"]);
    try {
      seedBankTransaction(ws, "acme-aps", "2025-12-15", "Primo", 400, 400);
      seedBankTransaction(ws, "acme-aps", "2026-03-01", "Indbetaling", 600, 1000);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.openingBalance).toBe(400);
      expect(cf.closingBalance).toBe(1000);
      expect(cf.totalIn).toBe(600);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no bank transactions reports an empty cash flow", async () => {
    const ws = makeWorkspace("cf-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/cashflow?year=2026",
      );
      expect(res.status).toBe(200);
      const cf = res.body.cashflow;
      expect(cf.hasTransactions).toBe(false);
      expect(cf.totalIn).toBe(0);
      expect(cf.totalOut).toBe(0);
      expect(cf.balanceSeries).toEqual([]);
      expect(cf.months.length).toBe(12);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("cashflow for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("cf-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/cashflow",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
