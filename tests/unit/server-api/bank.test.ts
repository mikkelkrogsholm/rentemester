import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  makeWorkspace,
  postPnlEntry,
  rmSync,
  seedArchiveYear,
  seedBankTransaction,
} from "./_shared";

describe("cockpit API — bank (GET .../bank)", () => {
  test("returns transactions with reconciliation status and booked balance", async () => {
    const ws = makeWorkspace("bnk-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Bankgebyr", -50);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.slug).toBe("acme-aps");
      expect(b.transactions.length).toBe(1);
      expect(b.transactions[0].reconciliationStatus).toBe("unmatched");
      expect(b.unmatchedCount).toBe(1);
      expect(b).toHaveProperty("bookedBalance");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("reports the actual statement balance and the gap to booked", async () => {
    const ws = makeWorkspace("bnk-actual", ["Acme ApS"]);
    try {
      // Booked balance on account 2000 = 750 (see postPnlEntry).
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Indbetaling", 700, 700);
      seedBankTransaction(ws, "acme-aps", "2026-04-10", "Gebyr", -200, 500);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2026",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.bookedBalance).toBe(750);
      expect(b.actualBalance).toBe(500);
      expect(b.difference).toBe(250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("bank for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("bnk-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/bank",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("shows the imported bank transactions for an archived fiscal year", async () => {
    const ws = makeWorkspace("bnk-archived", ["Acme ApS"]);
    try {
      // 2024 is an archived year from a prior-system migration; the live
      // ledger's only year is 2026. But the owner's bank-statement CSV spans
      // both years — its 2024 rows are live, append-only data and must still
      // be shown when the owner selects the archived year.
      seedArchiveYear(ws, "acme-aps", 2024, [["2000", "Bank", 3800]]);
      seedBankTransaction(ws, "acme-aps", "2024-06-01", "Leverandørbetaling", -500, 4200);
      seedBankTransaction(ws, "acme-aps", "2024-09-15", "Kundeindbetaling", 1200, 5400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/bank?year=2024",
      );
      expect(res.status).toBe(200);
      const b = res.body.bank;
      expect(b.archived).toBe(true);
      expect(b.transactions.length).toBe(2);
      expect(b.transactions.map((t: { text: string }) => t.text)).toEqual([
        "Leverandørbetaling",
        "Kundeindbetaling",
      ]);
      // The statement's own running balance is valid for an archived year.
      expect(b.actualBalance).toBe(5400);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
