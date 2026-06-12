import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — VAT (GET .../vat)", () => {
  test("returns the output/input/payable VAT for the period", async () => {
    const ws = makeWorkspace("vat-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      const v = res.body.vat;
      expect(v.slug).toBe("acme-aps");
      expect(v.outputVat).toBe(250);
      expect(v.inputVat).toBe(100);
      expect(v.payable).toBe(150);
      // 2026-03-15 falls in Q1 — quarterly is the only VAT cadence.
      expect(v.periodLabel).toBe("Q1 2026");
      // The full SKAT TastSelv rubrics are surfaced so the owner can file
      // straight from the cockpit — salgsmoms/købsmoms/momstilsvar plus the
      // foreign-trade rubrics A/B/C, the same numbers `vat momsangivelse` gives.
      expect(v.rubrikker.salgsmoms).toBe(250);
      expect(v.rubrikker.kobsmoms).toBe(100);
      expect(v.rubrikker.momstilsvar).toBe(150);
      expect(v.rubrikker.momsAfVarekobUdland).toBe(0);
      expect(v.rubrikker.momsAfYdelseskobUdland).toBe(0);
      expect(v.rubrikker.rubrikA).toBe(0);
      expect(v.rubrikker.rubrikB).toBe(0);
      expect(v.rubrikker.rubrikC).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("vat for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("vat-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/vat",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
