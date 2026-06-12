import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  makeWorkspace,
  postBadDebtWriteoff,
  postPnlEntry,
  rmSync,
} from "./_shared";

// #271: a bad-debt write-off books a debit on the output-VAT account. The
// cockpit VAT card must not let that debit drag the headline salgsmoms
// negative — the adjustment belongs on its own clearly-labelled line.
describe("cockpit API — VAT bad-debt adjustment (#271)", () => {
  test("a write-off does not turn salgsmoms negative — it is its own line", async () => {
    const ws = makeWorkspace("vat-baddebt", ["Acme ApS"]);
    try {
      // Q2 2026: genuine sales of 1000 → 250 output VAT, plus a 400 purchase.
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      // Q2 2026: a bad-debt write-off whose VAT relief (250) is large enough
      // that a naive chart-of-accounts sum would net salgsmoms to exactly 0,
      // and a bigger write-off would push it negative. Use 1200 net → 300
      // relief so the booked output-VAT account sum is 250 − 300 = −50.
      postBadDebtWriteoff(ws, "acme-aps", "2026-05-20", 1200);

      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      const v = res.body.vat;
      // The headline salgsmoms is the genuine VAT on sales — never negative.
      expect(v.outputVat).toBe(250);
      // The bad-debt relief sits on its own line, as a negative adjustment.
      expect(v.outputVatAdjustment).toBe(-300);
      // The net payable still reflects the relief: 250 − 300 − 100 = −150.
      expect(v.payable).toBe(-150);
      expect(v.inputVat).toBe(100);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("with no write-off the adjustment line is a clean zero", async () => {
    const ws = makeWorkspace("vat-noadjust", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.vat.outputVat).toBe(250);
      expect(res.body.vat.outputVatAdjustment).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
