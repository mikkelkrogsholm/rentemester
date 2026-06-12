import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — VAT deadline (GET .../vat & .../overview)", () => {
  test("vat carries the statutory filing deadline and a countdown", async () => {
    const ws = makeWorkspace("vat-deadline", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      // Q1 2026 (Jan–Mar) → filed/paid by 1 June 2026.
      expect(res.body.vat.deadline).toBe("2026-06-01");
      expect(typeof res.body.vat.daysRemaining).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview's VAT card and receivables block carry the new fields", async () => {
    const ws = makeWorkspace("ov-deadline", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      // Q1 2026 (Jan–Mar) → filed/paid by 1 June 2026.
      expect(res.body.overview.vat.deadline).toBe("2026-06-01");
      // No issued invoices → a clean zero receivables block.
      expect(res.body.overview.receivables).toEqual({
        openCount: 0,
        openTotal: 0,
      });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
