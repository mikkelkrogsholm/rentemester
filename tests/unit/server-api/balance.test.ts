import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — balance sheet (GET .../balance)", () => {
  test("returns asset/liability/equity sections that balance", async () => {
    const ws = makeWorkspace("bal-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.slug).toBe("acme-aps");
      expect(b.asOfDate).toBe("2026-12-31");
      expect(b.balanced).toBe(true);
      expect(b.totalAssets).toBe(b.totalLiabilitiesAndEquity);
      expect(b.assets).toHaveProperty("lines");
      expect(b.assets).toHaveProperty("total");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("balance for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("bal-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/balance",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a prior-year posting surfaces a priorAmount on each balance line and a priorTotal on each section (#400)", async () => {
    const ws = makeWorkspace("bal-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2025-04-01", 800, 0);
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      // Every section carries a priorTotal — a number, not null — because a
      // prior live year exists in the ledger.
      expect(typeof b.assets.priorTotal).toBe("number");
      expect(typeof b.liabilities.priorTotal).toBe("number");
      expect(typeof b.equity.priorTotal).toBe("number");
      expect(typeof b.priorTotalLiabilitiesAndEquity).toBe("number");
      // The prior balance balances by definition (double-entry).
      expect(b.assets.priorTotal).toBeCloseTo(
        b.priorTotalLiabilitiesAndEquity,
        2,
      );
      // The synthetic "Årets resultat" line in equity carries last year's
      // result (800) as its priorAmount, not the current year's.
      const aretsResultat = b.equity.lines.find(
        (l: { name: string }) => l.name === "Årets resultat",
      );
      expect(aretsResultat).toBeTruthy();
      expect(aretsResultat.priorAmount).toBeCloseTo(800, 2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("when no prior year exists, every priorAmount and priorTotal is null (#400)", async () => {
    // A company in its first regnskabsår — the ledger has no foregående år,
    // so the prior column must be uniformly null rather than misleadingly 0.
    const ws = makeWorkspace("bal-no-prior", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-04-01", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.assets.priorTotal).toBeNull();
      expect(b.liabilities.priorTotal).toBeNull();
      expect(b.equity.priorTotal).toBeNull();
      expect(b.priorTotalLiabilitiesAndEquity).toBeNull();
      for (const l of b.assets.lines) expect(l.priorAmount).toBeNull();
      for (const l of b.liabilities.lines) expect(l.priorAmount).toBeNull();
      for (const l of b.equity.lines) expect(l.priorAmount).toBeNull();
      // The balanced-flag only covers the current year — see acceptkriterier.
      expect(b.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
