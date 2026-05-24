// Tests: src/cli/init.ts, src/cli.ts (init CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { loadActorAllowlist } from "../../src/cli-actor";

describe("init CLI", () => {
  test("stores fiscal-year and CVR company configuration", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-cli-"));
    const company = join(root, "company");

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "init",
      "--company", company,
      "--cvr", "DK12345678",
      "--fiscal-year-start-month", "7",
      "--fiscal-year-label-strategy", "span",
      // #241: passing bank details keeps stderr clean (no advisory warning).
      "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });

    const db = openDb(companyPaths(company).db);
    migrate(db);
    const row = db.query(
      `SELECT cvr, fiscal_year_start_month, fiscal_year_label_strategy
         FROM companies
        WHERE id = 1`
    ).get() as any;

    expect(row).toEqual({
      cvr: "DK12345678",
      fiscal_year_start_month: 7,
      fiscal_year_label_strategy: "span",
    });

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("registers an init-created company in the workspace so the Cockpit sees it (#216)", async () => {
    const ws = mkdtempSync(join(tmpdir(), "rentemester-init-ws-"));
    try {
      const company = join(ws, "acme-aps");
      const initProc = Bun.spawn(
        [
          "bun", "run", "src/cli.ts", "init", "--company", company, "--format", "json",
          // #241: pass bank details so the advisory warning on stderr does not
          // confuse this test, which is about workspace registration, not #241.
          "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
        ],
        {
          cwd: process.cwd(),
          stdout: "pipe",
          stderr: "pipe",
          env: { ...process.env, RENTEMESTER_WORKSPACE: ws, RENTEMESTER_COMPANY: "" },
        },
      );
      const stdout = await new Response(initProc.stdout).text();
      const stderr = await new Response(initProc.stderr).text();
      const exitCode = await initProc.exited;
      expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });

      // The workspace manifest now lists the company.
      const manifest = JSON.parse(await Bun.file(join(ws, "workspace.json")).text());
      expect(manifest.companies.map((c: any) => c.slug)).toEqual(["acme-aps"]);

      // JSON output exposes the registration outcome for agents.
      const result = JSON.parse(stdout);
      expect(result.workspaceRegistered).toBe(true);
      expect(result.workspaceSlug).toBe("acme-aps");

      // `company list` (the Cockpit's view) now finds it too.
      const listProc = Bun.spawn(
        ["bun", "run", "src/cli.ts", "company", "list", "--format", "json"],
        {
          cwd: process.cwd(),
          stdout: "pipe",
          stderr: "pipe",
          env: { ...process.env, RENTEMESTER_WORKSPACE: ws, RENTEMESTER_COMPANY: "" },
        },
      );
      const listStdout = await new Response(listProc.stdout).text();
      await listProc.exited;
      expect(JSON.parse(listStdout).count).toBe(1);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a raw --company path outside any workspace is not registered (#216 back-compat)", async () => {
    const ws = mkdtempSync(join(tmpdir(), "rentemester-init-ws-out-"));
    const elsewhere = mkdtempSync(join(tmpdir(), "rentemester-init-elsewhere-"));
    try {
      const company = join(elsewhere, "company");
      const proc = Bun.spawn(
        ["bun", "run", "src/cli.ts", "init", "--company", company, "--format", "json"],
        {
          cwd: process.cwd(),
          stdout: "pipe",
          stderr: "pipe",
          env: { ...process.env, RENTEMESTER_WORKSPACE: ws, RENTEMESTER_COMPANY: "" },
        },
      );
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      expect(JSON.parse(stdout).workspaceRegistered).toBe(false);
      // The workspace manifest is untouched (not created by this init).
      expect(JSON.parse(stdout).workspaceSlug).toBeUndefined();
    } finally {
      rmSync(ws, { recursive: true, force: true });
      rmSync(elsewhere, { recursive: true, force: true });
    }
  });

  test("human output shows an onboarding summary and next steps (#214)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-onboard-"));
    try {
      const company = join(root, "company");
      const proc = Bun.spawn(
        ["bun", "run", "src/cli.ts", "init", "--company", company, "--format", "human"],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      // Summary of what was created, including the seeded chart of accounts.
      expect(stdout).toContain("Standardkontoplan");
      expect(stdout).toMatch(/\d+ konti/);
      // The settings that matter are surfaced for confirmation.
      expect(stdout).toContain("Regnskabsår");
      expect(stdout).toContain("Momsperiode");
      expect(stdout.toLowerCase()).toContain("kvartal");
      // A clear next-steps path.
      expect(stdout).toContain("Næste skridt");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("--format json output stays machine-stable for agents (#214)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-json-"));
    try {
      const company = join(root, "company");
      const proc = Bun.spawn(
        ["bun", "run", "src/cli.ts", "init", "--company", company, "--format", "json"],
        { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
      );
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      // The whole stdout parses as a single JSON object — no prose mixed in.
      const result = JSON.parse(stdout);
      expect(result.ok).toBe(true);
      expect(result.companyRoot).toBe(company);
      expect(result.accountCount).toBeGreaterThan(0);
      expect(result.fiscalYearStartMonth).toBe(1);
      // #289: vatPeriod is the canonical period-type value; quarterly default.
      expect(result.vatPeriod).toBe("quarter");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #231: a freshly init'ed company must ship an actor_allowlist so an
  // explicit --actor works out of the box without hand-editing policy.yaml.
  test("seeds a usable actor_allowlist in the default policy.yaml", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-allowlist-"));
    const company = join(root, "company");
    try {
      const proc = Bun.spawn(["bun", "run", "src/cli.ts", "init", "--company", company], {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
      });
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);

      const policy = readFileSync(join(companyPaths(company).config, "policy.yaml"), "utf8");
      expect(policy).toContain("actor_allowlist:");

      const allowlist = loadActorAllowlist(company);
      expect(allowlist.has("agent:rentemester-bookkeeper")).toBe(true);
      expect(allowlist.has("user:ejer")).toBe(true);
      expect(allowlist.has("system:rentemester")).toBe(true);
      // An actor that was never seeded is still rejected.
      expect(allowlist.has("agent:freja")).toBe(false);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #289: the VAT period type (month/quarter/half-year) must be settable at
  // init and stored on the company profile, so a company that files monthly or
  // half-yearly VAT is no longer stuck with the quarterly assumption.
  test("--vat-period half-year stores half-yearly VAT and surfaces it in output", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-vat-period-"));
    const company = join(root, "company");
    try {
      const proc = Bun.spawn([
        "bun", "run", "src/cli.ts", "init",
        "--company", company,
        "--vat-period", "half-year",
        "--format", "json",
        // #241: bank details keep stderr clean — this test asserts VAT period
        // handling, not the missing-bank-details advisory.
        "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
      ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
      const stdout = await new Response(proc.stdout).text();
      const stderr = await new Response(proc.stderr).text();
      const exitCode = await proc.exited;
      expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });

      // JSON output reports the chosen cadence, not the quarterly default.
      const result = JSON.parse(stdout);
      expect(result.vatPeriod).toBe("half-year");

      // The cadence is stored on the company row so reads stay consistent.
      const db = openDb(companyPaths(company).db);
      migrate(db);
      const row = db.query(
        `SELECT vat_period_type FROM companies WHERE id = 1`,
      ).get() as any;
      expect(row.vat_period_type).toBe("half-year");
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #289: default cadence stays quarterly so existing companies are unaffected.
  test("init without --vat-period defaults to quarterly VAT", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-vat-default-"));
    const company = join(root, "company");
    try {
      const proc = Bun.spawn([
        "bun", "run", "src/cli.ts", "init", "--company", company, "--format", "json",
      ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      expect(JSON.parse(stdout).vatPeriod).toBe("quarter");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #289: the onboarding help must point at the --vat-period flag instead of
  // giving advice the owner cannot follow.
  test("human onboarding advice points at the --vat-period flag (#289)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-vat-advice-"));
    const company = join(root, "company");
    try {
      const proc = Bun.spawn([
        "bun", "run", "src/cli.ts", "init",
        "--company", company,
        "--vat-period", "month",
        "--format", "human",
      ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      expect(exitCode).toBe(0);
      // The cadence shown reflects the chosen setting.
      expect(stdout).toContain("Momsperiode");
      expect(stdout.toLowerCase()).toContain("måned");
      // The advice is now actionable: it names the flag that changes it.
      expect(stdout).toContain("--vat-period");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("does not double-prefix DK in stored CVR or dashboard output", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-init-cli-cvr-"));
    const company = join(root, "company");
    const dashboardOut = join(root, "dashboard.html");

    const initProc = Bun.spawn([
      "bun", "run", "src/cli.ts", "init",
      "--company", company,
      "--cvr", "DK12345678",
      // #241: bank details keep stderr clean for this CVR-format assertion.
      "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const initStderr = await new Response(initProc.stderr).text();
    const initExit = await initProc.exited;
    expect({ exitCode: initExit, stderr: initStderr }).toEqual({ exitCode: 0, stderr: "" });

    const db = openDb(companyPaths(company).db);
    migrate(db);
    const row = db.query(`SELECT cvr FROM companies WHERE id = 1`).get() as any;
    expect(row.cvr).toBe("DK12345678");
    expect(row.cvr).not.toContain("DKDK");
    db.close();

    const dashProc = Bun.spawn([
      "bun", "run", "src/cli.ts", "dashboard",
      "--company", company,
      "--out", dashboardOut,
      "--as-of", "2026-05-17",
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const dashStderr = await new Response(dashProc.stderr).text();
    const dashExit = await dashProc.exited;
    expect({ exitCode: dashExit, stderr: dashStderr }).toEqual({ exitCode: 0, stderr: "" });

    const html = await Bun.file(dashboardOut).text();
    expect(html).toContain("CVR DK12345678");
    expect(html).not.toContain("DKDK");

    rmSync(root, { recursive: true, force: true });
  });
});
