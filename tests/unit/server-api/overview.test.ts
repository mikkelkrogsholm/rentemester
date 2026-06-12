import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  makeWorkspace,
  postPnlEntry,
  rmSync,
  seedBankTransaction,
  seedException,
} from "./_shared";

describe("cockpit API — overview (GET /api/companies/:slug/overview)", () => {
  test("returns the P&L, bank and VAT blocks for the live year", async () => {
    const ws = makeWorkspace("ov-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.overview.slug).toBe("acme-aps");
      expect(res.body.overview.selectedYear).toBe("2026");
      expect(res.body.overview.archived).toBe(false);
      expect(res.body.overview.profitAndLoss.omsaetning).toBe(1000);
      expect(res.body.overview.profitAndLoss.udgifter).toBe(400);
      expect(res.body.overview.profitAndLoss.resultat).toBe(600);
      expect(res.body.overview.profitAndLoss.months).toHaveLength(12);
      expect(res.body.overview.profitAndLoss.months[2].income).toBe(1000);
      expect(res.body.overview.profitAndLoss.months[2].expense).toBe(400);
      // VAT: 25% of the 1000 sales base / the 400 purchase base. The P&L
      // entry is dated 2026-03-15 (Q1), so the surfaced quarter is Q1 2026 —
      // quarterly is the only VAT cadence, consistent with the dashboard/CLI.
      expect(res.body.overview.vat.outputVat).toBe(250);
      expect(res.body.overview.vat.inputVat).toBe(100);
      expect(res.body.overview.vat.payable).toBe(150);
      expect(res.body.overview.vat.periodLabel).toBe("Q1 2026");
      // Bank account 2000 nets +1250 (sale) −500 (purchase) = 750.
      expect(res.body.overview.bank.balance).toBe(750);
      expect(res.body.overview.recentEntries.length).toBeGreaterThan(0);
      expect(res.body.overview.fiscalYears[0].label).toBe("2026");
      // "Senest bogført" — the most recent posted transaction date.
      expect(res.body.overview.lastPostedDate).toBe("2026-03-15");
      // Nøgletal: bruttomargin = resultat ÷ omsætning = 600/1000 = 0.6.
      expect(res.body.overview.keyFigures.bruttomargin).toBeCloseTo(0.6, 6);
      // Egenkapitalandel is a fraction (0–1) when the balance has assets.
      expect(
        typeof res.body.overview.keyFigures.egenkapitalandel,
      ).toBe("number");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("defaults to the most recent live year when year is omitted", async () => {
    const ws = makeWorkspace("ov-default", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-02-01", 500, 100);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview",
      );
      expect(res.status).toBe(200);
      expect(res.body.overview.selectedYear).toBe("2026");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid year query value is a safe 400", async () => {
    const ws = makeWorkspace("ov-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("ov-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/overview",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("bank block reports actual balance and the gap to the booked balance", async () => {
    const ws = makeWorkspace("ov-bank-actual", ["Acme ApS"]);
    try {
      // Booked balance on account 2000 nets +1250 (sale) −500 (purchase) = 750.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      // Statement closes at 500 — short of the booked 750 by 250.
      seedBankTransaction(ws, "acme-aps", "2026-04-01", "Indbetaling", 700, 700);
      seedBankTransaction(ws, "acme-aps", "2026-04-10", "Gebyr", -200, 500);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const bank = res.body.overview.bank;
      expect(bank.balance).toBe(750);
      expect(bank.actualBalance).toBe(500);
      expect(bank.difference).toBe(250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("groups same-type exceptions into one Danish summary line", async () => {
    const ws = makeWorkspace("ov-exc-group", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 1 unmatched");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 2 unmatched");
      seedException(ws, "acme-aps", "UNMATCHED_BANK_TRANSACTION", "Bank transaction 3 unmatched");
      seedException(ws, "acme-aps", "MAIL_INTAKE_NO_ATTACHMENT", "Mail without attachment");
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const exc = res.body.overview.exceptions;
      expect(exc.count).toBe(4);
      // Two groups: 3 bank rows + 1 mail row, each one line.
      expect(exc.groups.length).toBe(2);
      const bankGroup = exc.groups.find(
        (g: { type: string }) => g.type === "UNMATCHED_BANK_TRANSACTION",
      );
      expect(bankGroup.count).toBe(3);
      expect(bankGroup.label).toContain("3 banktransaktioner");
      expect(bankGroup.link).toBe("bank");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("each overview exception row carries its requiredAction guidance (#254)", async () => {
    const ws = makeWorkspace("ov-exc-action", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      seedException(
        ws,
        "acme-aps",
        "UNMATCHED_BANK_TRANSACTION",
        "Banktransaktion 12 mangler afstemning",
        "Find bilaget for indbetalingen og bogfør den som indtægt.",
      );
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(res.status).toBe(200);
      const row = res.body.overview.exceptions.rows[0];
      expect(row.message).toBe("Banktransaktion 12 mangler afstemning");
      // The concrete action — the most useful part — is on the wire (#254).
      expect(row.requiredAction).toBe(
        "Find bilaget for indbetalingen og bogfør den som indtægt.",
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
