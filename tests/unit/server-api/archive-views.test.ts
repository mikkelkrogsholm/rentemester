import { describe, expect, test } from "bun:test";
import {
  companyPaths,
  companyRootForSlug,
  config,
  get,
  makeWorkspace,
  migrate,
  openDb,
  rmSync,
  seedArchiveYear,
} from "./_shared";

// --------------------------------------------------------------------------
// Archive-aware core views (#197 — Runde 3, iteration 10)
//
// The core statement endpoints derive their figures from `import_archive_*`
// when the selected year is an archived one — the same chart of accounts
// classification the live ledger uses, applied to the archived SaldoBalance.
// --------------------------------------------------------------------------

describe("cockpit API — archive-aware core views (#197)", () => {
  test("income-statement classifies an archived year's SaldoBalance", async () => {
    const ws = makeWorkspace("arc-is", ["Acme ApS"]);
    try {
      // Archived 2024 — income 1000 closes at −5000 (credit), expense 3000 at
      // 1200 (debit). Resultat = 5000 − 1200 = 3800.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 1200],
        ["2000", "Bank", 3800],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/income-statement?year=2024",
      );
      expect(res.status).toBe(200);
      const is = res.body.incomeStatement;
      expect(is.archived).toBe(true);
      expect(is.archivedSource).toBe("dinero");
      expect(is.income).toHaveLength(1);
      expect(is.income[0].amount).toBe(5000);
      expect(is.expense).toHaveLength(1);
      expect(is.expense[0].amount).toBe(1200);
      expect(is.totalIncome).toBe(5000);
      expect(is.totalExpense).toBe(1200);
      expect(is.result).toBe(3800);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("balance classifies an archived year into asset/liability/equity", async () => {
    const ws = makeWorkspace("arc-bal", ["Acme ApS"]);
    try {
      // Assets 6000 debit (2000), liability 4500 credit (−1000 archive sign),
      // equity 5000 credit (−2000), income/expense net to the 3000 result.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["2000", "Bank", 6000],
        ["4500", "Momsafregning", -1000],
        ["5000", "Egenkapital", -2000],
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 2000],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/balance?year=2024",
      );
      expect(res.status).toBe(200);
      const b = res.body.balance;
      expect(b.archived).toBe(true);
      expect(b.totalAssets).toBe(6000);
      expect(b.liabilities.total).toBe(1000);
      // The 3000 period result is folded into equity as an "Årets resultat"
      // line, so equity.total is the equity-account sum (2000) plus the result.
      expect(b.periodResult).toBe(3000);
      const resultLine = b.equity.lines.find(
        (l: { name: string }) => l.name === "Årets resultat",
      );
      expect(resultLine?.amount).toBe(3000);
      expect(b.equity.total).toBe(5000);
      // The archived balance sheet balances: assets = liabilities + equity.
      expect(b.totalLiabilitiesAndEquity).toBe(6000);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);
      expect(b.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archived equity.total matches the Flerårsoversigt's egenkapital", async () => {
    const ws = makeWorkspace("arc-bal-consistency", ["Acme ApS"]);
    try {
      // A distressed year: a negative (overdrawn) bank, an equity deficit and
      // a loss. The archived SaldoBalance is debit-signed and sums to zero, so
      // the sheet must still balance and the two views must agree on equity.
      seedArchiveYear(ws, "acme-aps", 2023, [
        ["2000", "Bank", -3000],
        ["4500", "Momsafregning", -500],
        ["5000", "Egenkapital", 1500],
        ["1000", "Omsætning", -2000],
        ["3000", "Software", 4000],
      ]);
      const cfg = config({ workspaceRoot: ws });
      const balRes = await get(cfg, "/api/companies/acme-aps/balance?year=2023");
      expect(balRes.status).toBe(200);
      const b = balRes.body.balance;
      // The balance balances even for a distressed (negative-asset) year.
      expect(b.balanced).toBe(true);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);

      const myRes = await get(cfg, "/api/companies/acme-aps/multi-year");
      expect(myRes.status).toBe(200);
      const my2023 = myRes.body.multiYear.years.find(
        (r: { year: string }) => r.year === "2023",
      );
      // The Balance view and the Flerårsoversigt agree on equity.
      expect(my2023.egenkapital).toBe(b.equity.total);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archived vat accounts are classified by normal balance, consistently across Balance and Flerårsoversigt (#321)", async () => {
    const ws = makeWorkspace("arc-vat-class", ["Acme ApS"]);
    try {
      // The native-Rentemester chart types `4000` Købsmoms as `vat`/debit
      // (input VAT — a receivable, so an asset) and `1200` Salgsmoms as
      // `vat`/credit (output VAT — a payable, so a liability). An archived
      // SaldoBalance carrying both must place them by their normal balance:
      // `4000` under assets, `1200` under liabilities. The shared #321
      // classification guarantees the Balance view and the Flerårsoversigt
      // agree — before #321 the Flerårsoversigt left `vat` accounts
      // unclassified, so its `balancesum` silently dropped the `4000` asset.
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["2000", "Bank", 5000],
        ["4000", "Købsmoms", 1000], // vat/debit → an asset
        ["1200", "Salgsmoms", -2000], // vat/credit → a liability
        ["5000", "Egenkapital", -1000],
        ["1000", "Omsætning", -6000],
        ["3000", "Software", 3000],
      ]);
      const cfg = config({ workspaceRoot: ws });

      const balRes = await get(cfg, "/api/companies/acme-aps/balance?year=2024");
      expect(balRes.status).toBe(200);
      const b = balRes.body.balance;
      // The vat/debit Købsmoms is an asset: 5000 Bank + 1000 Købsmoms.
      expect(b.totalAssets).toBe(6000);
      // The vat/credit Salgsmoms is a liability: 2000.
      expect(b.liabilities.total).toBe(2000);
      // The sheet still balances: assets = liabilities + equity.
      expect(b.balanced).toBe(true);
      expect(b.liabilities.total + b.equity.total).toBe(b.totalAssets);

      const myRes = await get(cfg, "/api/companies/acme-aps/multi-year");
      expect(myRes.status).toBe(200);
      const my2024 = myRes.body.multiYear.years.find(
        (r: { year: string }) => r.year === "2024",
      );
      // The Flerårsoversigt's balancesum counts the vat/debit account as an
      // asset, exactly as the Balance view does — the two never disagree.
      expect(my2024.balancesum).toBe(b.totalAssets);
      expect(my2024.egenkapital).toBe(b.equity.total);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("trial-balance renders the archived SaldoBalance directly", async () => {
    const ws = makeWorkspace("arc-tb", ["Acme ApS"]);
    try {
      seedArchiveYear(ws, "acme-aps", 2024, [
        ["1000", "Omsætning", -5000],
        ["3000", "Software", 1200],
        ["2000", "Bank", 3800],
      ]);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/trial-balance?year=2024",
      );
      expect(res.status).toBe(200);
      const t = res.body.trialBalance;
      expect(t.archived).toBe(true);
      expect(t.rows).toHaveLength(3);
      const income = t.rows.find((r: any) => r.accountNo === "1000");
      expect(income.credit).toBe(5000);
      expect(income.debit).toBe(0);
      expect(t.totalDebit).toBe(5000);
      expect(t.totalCredit).toBe(5000);
      expect(t.balanced).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("journal groups archived postings by voucher", async () => {
    const ws = makeWorkspace("arc-jrn", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [["1000", "Omsætning", -5000]],
        [],
      );
      // Two postings share voucher "B-1"; one carries voucher "B-2".
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        const yearId = (
          db
            .query(
              "SELECT id FROM import_archive_years WHERE fiscal_year = 2024",
            )
            .get() as { id: number }
        ).id;
        const ins = db.prepare(
          `INSERT INTO import_archive_postings
             (archive_year_id, line_no, account_no, account_name,
              transaction_date, voucher, text, amount)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
        );
        ins.run(yearId, 0, "1000", "Omsætning", "2024-03-01", "B-1", "Salg", -5000);
        ins.run(yearId, 1, "2000", "Bank", "2024-03-01", "B-1", "Salg", 5000);
        ins.run(yearId, 2, "3000", "Software", "2024-06-01", "B-2", "Køb", 1200);
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2024",
      );
      expect(res.status).toBe(200);
      const j = res.body.journal;
      expect(j.archived).toBe(true);
      expect(j.entries).toHaveLength(2);
      // Newest first — B-2 (June) before B-1 (March).
      expect(j.entries[0].entryNo).toBe("B-2");
      const b1 = j.entries.find((e: any) => e.entryNo === "B-1");
      expect(b1.lines).toHaveLength(2);
      expect(b1.total).toBe(5000);
      // The ?account= drill-down filters archived entries too.
      const filtered = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2024&account=3000",
      );
      expect(filtered.body.journal.entries).toHaveLength(1);
      expect(filtered.body.journal.accountFilter.accountNo).toBe("3000");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("overview derives a P&L overview for an archived year", async () => {
    const ws = makeWorkspace("arc-ov", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [
          ["1000", "Omsætning", -5000],
          ["3000", "Software", 1200],
        ],
        [],
      );
      const db = openDb(companyPaths(companyRootForSlug(ws, "acme-aps")).db);
      try {
        migrate(db);
        const yearId = (
          db
            .query(
              "SELECT id FROM import_archive_years WHERE fiscal_year = 2024",
            )
            .get() as { id: number }
        ).id;
        const ins = db.prepare(
          `INSERT INTO import_archive_postings
             (archive_year_id, line_no, account_no, account_name,
              transaction_date, voucher, text, amount)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
        );
        ins.run(yearId, 0, "1000", "Omsætning", "2024-03-10", "B-1", "Salg", -5000);
        ins.run(yearId, 1, "3000", "Software", "2024-03-10", "B-1", "Køb", 1200);
      } finally {
        db.close();
      }
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2024",
      );
      expect(res.status).toBe(200);
      const o = res.body.overview;
      expect(o.archived).toBe(true);
      expect(o.archivedSource).toBe("dinero");
      expect(o.profitAndLoss.omsaetning).toBe(5000);
      expect(o.profitAndLoss.udgifter).toBe(1200);
      expect(o.profitAndLoss.resultat).toBe(3800);
      expect(o.profitAndLoss.months).toHaveLength(12);
      // March (index 2) carries the bucketed activity.
      expect(o.profitAndLoss.months[2].income).toBe(5000);
      expect(o.profitAndLoss.months[2].expense).toBe(1200);
      // Live-only sections are honestly N/A, not faked.
      expect(o.vat).toBeNull();
      expect(o.bank.actualBalance).toBeNull();
      expect(o.recentEntries.length).toBeGreaterThan(0);
      expect(o.lastPostedDate).toBe("2024-03-10");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
