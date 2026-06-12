// Tests: the VAT-period-type feature (#299/#300/#301/#303) — `vatPeriodType`
// (month/quarter/half-year) drives the VAT period, label and deadline across
// the periods core, the static + CLI dashboard, the cockpit data builders and
// the `company set-profile`/`company profile`/PATCH-profile/reopen surfaces.
//
// `quarter` is the historical default — every quarter assertion below doubles
// as a byte-identical back-compat check.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  vatPeriodLabel,
  vatPeriodsForYear,
  vatPeriodWindowFor,
  setCompanyVatPeriodType,
} from "../../src/core/periods";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { createCompany } from "../../src/core/company";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import {
  buildCompanyVat,
  buildCompanyObligations,
  buildCompanyOverview,
  buildCompanyDashboardData,
} from "../../src/server/data";
import { handleRequest } from "../../src/server/router";
import type { ServerConfig } from "../../src/server/config";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, RENTEMESTER_COMPANY: "", RENTEMESTER_WORKSPACE: "" },
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

/** A workspace with one company on the given VAT cadence. */
function makeWorkspace(label: string, vatPeriodType: string) {
  const root = tmpRoot(label);
  initWorkspace(root);
  const created = createCompany(root, { name: "Acme ApS", vatPeriodType });
  return { root, slug: created.slug };
}

/**
 * Books output VAT (a credit on the `vat`-type account `1200`) on `date` so a
 * VAT period carrying that date has activity and a positive payable.
 */
function postVatSale(ws: string, slug: string, date: string, vatAmount = 250) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const res = postJournalEntry(db, {
      transactionDate: date,
      text: "Salg med moms",
      lines: [
        { accountNo: "2000", debitAmount: vatAmount },
        { accountNo: "1200", creditAmount: vatAmount },
      ],
    });
    if (!res.ok) throw new Error(res.errors.join("; "));
  } finally {
    db.close();
  }
}

function config(workspaceRoot: string): ServerConfig {
  return { host: "127.0.0.1", port: 0, authRequired: false, authToken: null, workspaceRoot };
}

async function call(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  return { status: res.status, body: await res.json() };
}

async function patch(cfg: ServerConfig, path: string, body: unknown) {
  return call(cfg, path, {
    method: "PATCH",
    headers: { host: "127.0.0.1" },
    body: JSON.stringify(body),
  });
}

async function post(cfg: ServerConfig, path: string, body?: unknown) {
  const init: RequestInit = { method: "POST", headers: { host: "127.0.0.1" } };
  if (body !== undefined) init.body = JSON.stringify(body);
  return call(cfg, path, init);
}

// --------------------------------------------------------------------------
// #299 — periods-core helpers: label + per-year window enumeration.
// --------------------------------------------------------------------------
describe("periods core — vatPeriodLabel / vatPeriodsForYear (#299)", () => {
  test("vatPeriodLabel reads naturally for each cadence", () => {
    expect(vatPeriodLabel(vatPeriodWindowFor("2026-05-22", "quarter"))).toBe(
      "Q2 2026",
    );
    expect(vatPeriodLabel(vatPeriodWindowFor("2026-05-22", "month"))).toBe(
      "Maj 2026",
    );
    expect(vatPeriodLabel(vatPeriodWindowFor("2026-02-10", "half-year"))).toBe(
      "1. halvår 2026",
    );
    expect(vatPeriodLabel(vatPeriodWindowFor("2026-11-03", "half-year"))).toBe(
      "2. halvår 2026",
    );
  });

  test("vatPeriodsForYear yields 12 / 4 / 2 chronological windows", () => {
    const months = vatPeriodsForYear(2026, "month");
    const quarters = vatPeriodsForYear(2026, "quarter");
    const halves = vatPeriodsForYear(2026, "half-year");
    expect(months.length).toBe(12);
    expect(quarters.length).toBe(4);
    expect(halves.length).toBe(2);
    // Quarterly back-compat: the four calendar quarters, in order.
    expect(quarters.map((w) => w.start)).toEqual([
      "2026-01-01",
      "2026-04-01",
      "2026-07-01",
      "2026-10-01",
    ]);
    // Half-yearly: two six-month windows with the right SKAT deadlines.
    expect(halves[0]!.start).toBe("2026-01-01");
    expect(halves[0]!.end).toBe("2026-06-30");
    expect(halves[0]!.filingDeadline).toBe("2026-09-01");
    expect(halves[1]!.start).toBe("2026-07-01");
    expect(halves[1]!.end).toBe("2026-12-31");
    expect(halves[1]!.filingDeadline).toBe("2027-03-01");
  });
});

// --------------------------------------------------------------------------
// #300 — setCompanyVatPeriodType writes / validates the cadence.
// --------------------------------------------------------------------------
describe("setCompanyVatPeriodType (#300)", () => {
  test("writes the cadence and reports whether it changed", () => {
    const { root: ws, slug } = makeWorkspace("set-vat-core", "quarter");
    try {
      const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
      try {
        migrate(db);
        const first = setCompanyVatPeriodType(db, "half-year");
        expect(first.ok).toBe(true);
        expect(first.changed).toBe(true);
        // A no-op write is `ok` but `changed: false`.
        const again = setCompanyVatPeriodType(db, "half-year");
        expect(again.ok).toBe(true);
        expect(again.changed).toBe(false);
        const stored = db
          .query("SELECT vat_period_type AS t FROM companies WHERE id = 1")
          .get() as { t: string };
        expect(stored.t).toBe("half-year");
      } finally {
        db.close();
      }
    } finally {
      cleanupDir(ws);
    }
  });

  test("rejects an unknown cadence", () => {
    const { root: ws, slug } = makeWorkspace("set-vat-bad", "quarter");
    try {
      const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
      try {
        migrate(db);
        const bad = setCompanyVatPeriodType(db, "yearly" as never);
        expect(bad.ok).toBe(false);
        expect(bad.errors[0]).toContain("month, quarter, half-year");
      } finally {
        db.close();
      }
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #299 — the static dashboard's "Næste momsfrist" follows the cadence.
// --------------------------------------------------------------------------
describe("static dashboard — VAT period follows the cadence (#299)", () => {
  test("a half-year company sees a half-year VAT period, not a quarter", () => {
    const { root: ws, slug } = makeWorkspace("dash-half", "half-year");
    try {
      // Activity in the first half of 2026.
      postVatSale(ws, slug, "2026-03-15");
      const data = buildCompanyDashboardData(ws, slug, "2026-05-17");
      // The selected VAT period is the Jan–Jun half, not Q1/Q2.
      expect(data.vat.periodStart).toBe("2026-01-01");
      expect(data.vat.periodEnd).toBe("2026-06-30");
    } finally {
      cleanupDir(ws);
    }
  });

  test("a quarter company is unchanged — Q1 window", () => {
    const { root: ws, slug } = makeWorkspace("dash-quarter", "quarter");
    try {
      postVatSale(ws, slug, "2026-02-15");
      const data = buildCompanyDashboardData(ws, slug, "2026-05-17");
      expect(data.vat.periodStart).toBe("2026-01-01");
      expect(data.vat.periodEnd).toBe("2026-03-31");
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #299 — the cockpit VAT card + obligations follow the cadence.
// --------------------------------------------------------------------------
describe("cockpit VAT card + obligations — cadence-aware (#299)", () => {
  test("VAT card of a half-year company shows a half-year label", () => {
    const { root: ws, slug } = makeWorkspace("vat-card-half", "half-year");
    try {
      postVatSale(ws, slug, "2026-03-15");
      const vat = buildCompanyVat(ws, slug, 2026);
      expect(vat.periodLabel).toBe("1. halvår 2026");
      expect(vat.periodStart).toBe("2026-01-01");
      expect(vat.periodEnd).toBe("2026-06-30");
      // Filing deadline = 1st of the 3rd month after the half ends.
      expect(vat.deadline).toBe("2026-09-01");
    } finally {
      cleanupDir(ws);
    }
  });

  test("VAT card of a quarter company is byte-identical to before", () => {
    const { root: ws, slug } = makeWorkspace("vat-card-quarter", "quarter");
    try {
      postVatSale(ws, slug, "2026-05-15");
      const vat = buildCompanyVat(ws, slug, 2026);
      expect(vat.periodLabel).toBe("Q2 2026");
      expect(vat.periodStart).toBe("2026-04-01");
      expect(vat.periodEnd).toBe("2026-06-30");
      expect(vat.deadline).toBe("2026-09-01");
    } finally {
      cleanupDir(ws);
    }
  });

  test("the cockpit overview VAT block uses the cadence label", () => {
    const { root: ws, slug } = makeWorkspace("overview-half", "half-year");
    try {
      // Activity in the first half — today (May) falls in that half.
      postVatSale(ws, slug, "2026-03-15");
      const overview = buildCompanyOverview(ws, slug, 2026);
      expect(overview.vat.periodLabel).toBe("1. halvår 2026");
    } finally {
      cleanupDir(ws);
    }
  });

  test("obligations of a half-year company list half-year VAT lines, never quarters", () => {
    const { root: ws, slug } = makeWorkspace("oblig-half", "half-year");
    try {
      // VAT activity in the first half of 2026 (a posting in the second half
      // would fall past the future-date guard).
      postVatSale(ws, slug, "2026-02-15");
      postVatSale(ws, slug, "2026-05-15");
      const obligations = buildCompanyObligations(ws, slug, 2026);
      const vatRows = obligations.obligations.filter((r) => r.kind === "vat");
      // The two postings sit in the SAME half-year period, so they collapse to
      // a single VAT obligation line — never two separate quarterly lines.
      expect(vatRows.length).toBe(1);
      expect(vatRows[0]!.label).toBe("Moms — 1. halvår 2026");
      // The half-year filing deadline, not a quarter's.
      expect(vatRows[0]!.dueDate).toBe("2026-09-01");
    } finally {
      cleanupDir(ws);
    }
  });

  test("obligations of a quarter company still list quarterly VAT lines", () => {
    const { root: ws, slug } = makeWorkspace("oblig-quarter", "quarter");
    try {
      postVatSale(ws, slug, "2026-02-15");
      postVatSale(ws, slug, "2026-05-15");
      const obligations = buildCompanyObligations(ws, slug, 2026);
      const vatRows = obligations.obligations.filter((r) => r.kind === "vat");
      expect(vatRows.map((r) => r.label)).toEqual([
        "Moms — Q1 2026",
        "Moms — Q2 2026",
      ]);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #303 — an OPEN VAT period's momsangivelse is provisional, not filing-ready.
// --------------------------------------------------------------------------
describe("cockpit VAT card — open period is provisional (#303)", () => {
  test("an unclosed period reports periodStatus 'open' and is not ready", () => {
    const { root: ws, slug } = makeWorkspace("vat-open", "quarter");
    try {
      postVatSale(ws, slug, "2026-02-15");
      const vat = buildCompanyVat(ws, slug, 2026);
      expect(vat.periodStatus).toBe("open");
      expect(vat.momsangivelseReady).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("a closed period reports periodStatus 'closed' and is filing-ready", async () => {
    const { root: ws, slug } = makeWorkspace("vat-closed", "quarter");
    try {
      postVatSale(ws, slug, "2026-02-15");
      // Close Q1 via the cockpit endpoint.
      const closed = await post(
        config(ws),
        `/api/companies/${slug}/periods/close`,
        { periodStart: "2026-01-01", periodEnd: "2026-03-31", confirm: true },
      );
      expect(closed.status).toBe(200);
      const vat = buildCompanyVat(ws, slug, 2026);
      expect(vat.periodStatus).toBe("closed");
      expect(vat.momsangivelseReady).toBe(true);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #300 — `company set-profile --vat-period` round-trips; `company profile`
// surfaces the cadence.
// --------------------------------------------------------------------------
describe("CLI company set-profile / profile — VAT cadence (#300)", () => {
  test("set-profile --vat-period round-trips and profile shows it", async () => {
    const root = tmpRoot("cli-vat-period");
    try {
      const company = join(root, "company");
      const initRes = await runCli(["init", "--company", company]);
      expect(initRes.exitCode).toBe(0);

      const setRes = await runCli([
        "company", "set-profile",
        "--company", company,
        "--vat-period", "half-year",
        "--format", "json",
      ]);
      expect({ exitCode: setRes.exitCode, stderr: setRes.stderr }).toEqual({
        exitCode: 0,
        stderr: "",
      });
      const setParsed = JSON.parse(setRes.stdout);
      expect(setParsed.ok).toBe(true);
      expect(setParsed.vatPeriodType).toBe("half-year");

      const profileRes = await runCli([
        "company", "profile",
        "--company", company,
        "--format", "json",
      ]);
      expect(profileRes.exitCode).toBe(0);
      const profile = JSON.parse(profileRes.stdout);
      expect(profile.profile.vatPeriodType).toBe("half-year");
      // The Danish label is surfaced alongside the canonical value.
      expect(profile.profile.vatPeriodLabel).toBe("halvår");
    } finally {
      cleanupDir(root);
    }
  });

  test("set-profile rejects an unknown --vat-period value with exit 1", async () => {
    const root = tmpRoot("cli-vat-period-bad");
    try {
      const company = join(root, "company");
      expect((await runCli(["init", "--company", company])).exitCode).toBe(0);
      const setRes = await runCli([
        "company", "set-profile",
        "--company", company,
        "--vat-period", "yearly",
        "--format", "json",
      ]);
      expect(setRes.exitCode).toBe(1);
      const parsed = JSON.parse(setRes.stdout);
      expect(parsed.ok).toBe(false);
      expect(parsed.errors[0]).toContain("month, quarter, half-year");
    } finally {
      cleanupDir(root);
    }
  });
});

// --------------------------------------------------------------------------
// #300 — the cockpit PATCH-profile endpoint accepts `vatPeriodType`.
// --------------------------------------------------------------------------
describe("cockpit PATCH profile — vatPeriodType (#300)", () => {
  test("PATCH .../company sets the cadence; it round-trips on GET", async () => {
    const { root: ws, slug } = makeWorkspace("patch-vat", "quarter");
    try {
      const res = await patch(config(ws), `/api/companies/${slug}/company`, {
        vatPeriodType: "half-year",
      });
      expect(res.status).toBe(200);
      expect(res.body.ok).toBe(true);
      expect(res.body.company.vatPeriodType).toBe("half-year");

      const fresh = await call(config(ws), `/api/companies/${slug}/company`);
      expect(fresh.body.company.vatPeriodType).toBe("half-year");
    } finally {
      cleanupDir(ws);
    }
  });

  test("PATCH .../company rejects an invalid vatPeriodType with a 400", async () => {
    const { root: ws, slug } = makeWorkspace("patch-vat-bad", "quarter");
    try {
      const res = await patch(config(ws), `/api/companies/${slug}/company`, {
        vatPeriodType: "yearly",
      });
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #301 — the cockpit can reopen a closed VAT period.
// --------------------------------------------------------------------------
describe("cockpit reopen period (#301)", () => {
  test("POST .../periods/reopen reopens a closed period; it becomes open", async () => {
    const { root: ws, slug } = makeWorkspace("reopen-ok", "quarter");
    try {
      postVatSale(ws, slug, "2026-02-15");
      const closed = await post(
        config(ws),
        `/api/companies/${slug}/periods/close`,
        { periodStart: "2026-01-01", periodEnd: "2026-03-31", confirm: true },
      );
      expect(closed.status).toBe(200);

      const reopened = await post(
        config(ws),
        `/api/companies/${slug}/periods/reopen`,
        {
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          reason: "bilag bogført for sent",
          confirm: true,
        },
      );
      expect(reopened.status).toBe(200);
      expect(reopened.body.ok).toBe(true);
      expect(reopened.body.period.effectiveStatus).toBe("open");

      // The VAT card now sees the period as open again — provisional figures.
      const vat = buildCompanyVat(ws, slug, 2026);
      expect(vat.periodStatus).toBe("open");
      expect(vat.momsangivelseReady).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("reopen without confirm is a 400", async () => {
    const { root: ws, slug } = makeWorkspace("reopen-noconfirm", "quarter");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/periods/reopen`,
        {
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          reason: "noget",
        },
      );
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
    } finally {
      cleanupDir(ws);
    }
  });

  test("reopening a period that was never closed is rejected", async () => {
    const { root: ws, slug } = makeWorkspace("reopen-missing", "quarter");
    try {
      const res = await post(
        config(ws),
        `/api/companies/${slug}/periods/reopen`,
        {
          periodStart: "2026-01-01",
          periodEnd: "2026-03-31",
          kind: "vat_quarter",
          reason: "fejl",
          confirm: true,
        },
      );
      // A reopen of a period that was never created is a bad-input rejection
      // (the request references a non-existent period), surfaced as a 400 by
      // `withCompanyMutation`'s core-result mapping.
      expect(res.status).toBe(400);
      expect(res.body.ok).toBe(false);
      expect(JSON.stringify(res.body)).toContain("no vat_quarter period");
    } finally {
      cleanupDir(ws);
    }
  });

  test("a GET on the reopen route is 405 — it is POST-only", async () => {
    const { root: ws, slug } = makeWorkspace("reopen-405", "quarter");
    try {
      const res = await call(config(ws), `/api/companies/${slug}/periods/reopen`);
      expect(res.status).toBe(405);
    } finally {
      cleanupDir(ws);
    }
  });
});

// --------------------------------------------------------------------------
// #299 — the CLI `dashboard` HTML shows the cadence's period label.
// --------------------------------------------------------------------------
describe("CLI dashboard — VAT period label follows the cadence (#299)", () => {
  test("a half-year company's dashboard HTML reads '1. halvår 2026'", async () => {
    const root = tmpRoot("cli-dash-half");
    try {
      const company = join(root, "company");
      const outPath = join(root, "dashboard.html");
      expect((await runCli(["init", "--company", company])).exitCode).toBe(0);
      expect(
        (
          await runCli([
            "company", "set-profile",
            "--company", company,
            "--vat-period", "half-year",
          ])
        ).exitCode,
      ).toBe(0);

      const dash = await runCli([
        "dashboard",
        "--company", company,
        "--out", outPath,
        "--as-of", "2026-05-17",
      ]);
      expect({ exitCode: dash.exitCode, stderr: dash.stderr }).toEqual({
        exitCode: 0,
        stderr: "",
      });
      const html = readFileSync(outPath, "utf8");
      // The "Næste momsfrist" box describes the half-year period, not a quarter.
      expect(html).toContain("1. halvår 2026");
      expect(html).not.toContain("Q2 2026");
    } finally {
      cleanupDir(root);
    }
  });

  test("a quarter company's dashboard HTML still reads 'Q2 2026'", async () => {
    const root = tmpRoot("cli-dash-quarter");
    try {
      const company = join(root, "company");
      const outPath = join(root, "dashboard.html");
      expect((await runCli(["init", "--company", company])).exitCode).toBe(0);
      const dash = await runCli([
        "dashboard",
        "--company", company,
        "--out", outPath,
        "--as-of", "2026-05-17",
      ]);
      expect(dash.exitCode).toBe(0);
      const html = readFileSync(outPath, "utf8");
      expect(html).toContain("Q2 2026");
    } finally {
      cleanupDir(root);
    }
  });
});
