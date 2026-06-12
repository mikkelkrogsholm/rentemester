import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  makeWorkspace,
  postPnlEntry,
  rmSync,
  seedArchiveYear,
} from "./_shared";

describe("cockpit API — multi-year (GET .../multi-year)", () => {
  test("returns key figures per year, oldest-first, live + archive", async () => {
    const ws = makeWorkspace("my-live", ["Acme ApS"]);
    try {
      // Archived 2025 — income account 1000 closes at −800, expense 3000 at 200.
      seedArchiveYear(ws, "acme-aps", 2025, [
        ["1000", "Omsætning", -800],
        ["3000", "Vareforbrug", 200],
      ]);
      // Live 2026.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const m = res.body.multiYear;
      expect(m.slug).toBe("acme-aps");
      expect(m.years.map((y: any) => y.year)).toEqual(["2025", "2026"]);
      const y2025 = m.years[0];
      expect(y2025.source).toBe("archive");
      expect(y2025.omsaetning).toBe(800);
      expect(y2025.udgifter).toBe(200);
      expect(y2025.resultat).toBe(600);
      const y2026 = m.years[1];
      expect(y2026.source).toBe("live");
      expect(y2026.omsaetning).toBe(1000);
      expect(y2026.udgifter).toBe(400);
      expect(y2026.resultat).toBe(600);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("returns balance-sheet figures and key ratios per year", async () => {
    const ws = makeWorkspace("my-balance", ["Acme ApS"]);
    try {
      // Archived 2025 — income −1000, expense 250, asset 2000 at 700, equity
      // 5000 closing at −150 (credit-signed −150 → +150 egenkapital section).
      // resultat = 1000 − 250 = 750; egenkapital = 150 + 750 = 900;
      // balancesum = 700; egenkapitalandel = 900 / 700.
      seedArchiveYear(ws, "acme-aps", 2025, [
        ["1000", "Omsætning", -1000],
        ["3000", "Vareforbrug", 250],
        ["2000", "Bank", 700],
        ["5000", "Egenkapital", -150],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const y2025 = res.body.multiYear.years[0];
      expect(y2025.balancesum).toBe(700);
      expect(y2025.egenkapital).toBe(900);
      expect(y2025.bruttomargin).toBeCloseTo(0.75, 5);
      expect(y2025.egenkapitalandel).toBeCloseTo(900 / 700, 5);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a ratio with a zero denominator is null, not a fabricated figure", async () => {
    const ws = makeWorkspace("my-ratio-null", ["Acme ApS"]);
    try {
      // No income and no assets — both ratios must be null.
      seedArchiveYear(ws, "acme-aps", 2025, [["3000", "Vareforbrug", 250]]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      const y2025 = res.body.multiYear.years[0];
      expect(y2025.bruttomargin).toBeNull();
      expect(y2025.egenkapitalandel).toBeNull();
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an empty ledger yields no years", async () => {
    const ws = makeWorkspace("my-empty", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/multi-year",
      );
      expect(res.status).toBe(200);
      expect(res.body.multiYear.years).toEqual([]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("multi-year for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("my-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/multi-year",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
