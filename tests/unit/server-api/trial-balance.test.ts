import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — trial balance (GET .../trial-balance)", () => {
  test("lists every moved account with debit, credit and balance", async () => {
    const ws = makeWorkspace("tb-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=2026",
      );
      expect(res.status).toBe(200);
      const tb = res.body.trialBalance;
      expect(tb.slug).toBe("acme-aps");
      expect(tb.balanced).toBe(true);
      expect(tb.totalDebit).toBe(tb.totalCredit);
      expect(tb.rows.length).toBeGreaterThan(0);
      expect(tb.rows[0]).toHaveProperty("accountNo");
      expect(tb.rows[0]).toHaveProperty("debit");
      expect(tb.rows[0]).toHaveProperty("credit");
      expect(tb.rows[0]).toHaveProperty("balance");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid year query value is a safe 400", async () => {
    const ws = makeWorkspace("tb-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("trial-balance for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("tb-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/trial-balance",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
