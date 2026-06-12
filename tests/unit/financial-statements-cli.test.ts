// Tests: src/cli/report.ts, src/cli.ts (financial statements CLI, #176)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
async function runReport(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { exitCode, stdout, stderr };
}

describe("report CLI", () => {
  test("emits trial balance, profit-loss and balance JSON for a company period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reportcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    // The fixture posts one expense entry: 3000 debit 1000, 4000 debit 250,
    // 2000 credit 1250 on 2026-05-16.
    const tb = await runReport(["report", "trial-balance", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"]);
    expect({ exitCode: tb.exitCode, stderr: tb.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const tbParsed = JSON.parse(tb.stdout);
    expect(tbParsed.ok).toBe(true);
    expect(tbParsed.balanced).toBe(true);
    expect(tbParsed.totalDebit).toBe(1250);
    expect(tbParsed.totalCredit).toBe(1250);
    const expense = tbParsed.accounts.find((a: { accountNo: string }) => a.accountNo === "3000");
    expect(expense.debit).toBe(1000);
    expect(expense.balance).toBe(1000);

    const pl = await runReport(["report", "profit-loss", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"]);
    expect({ exitCode: pl.exitCode, stderr: pl.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const plParsed = JSON.parse(pl.stdout);
    expect(plParsed.ok).toBe(true);
    expect(plParsed.totalIncome).toBe(0);
    expect(plParsed.totalExpense).toBe(1000);
    expect(plParsed.result).toBe(-1000);

    const bs = await runReport(["report", "balance", "--company", company, "--as-of", "2026-05-31"]);
    expect({ exitCode: bs.exitCode, stderr: bs.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const bsParsed = JSON.parse(bs.stdout);
    expect(bsParsed.ok).toBe(true);
    expect(bsParsed.balanced).toBe(true);
    expect(bsParsed.totalAssets).toBe(bsParsed.totalLiabilitiesAndEquity);
    expect(bsParsed.periodResult).toBe(-1000);

    cleanupDir(root);
  });

  test("fails with exit code 2 when a required flag is missing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reportcli-missing-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const tb = await runReport(["report", "trial-balance", "--company", company, "--from", "2026-05-01"]);
    expect(tb.exitCode).toBe(2);

    const bs = await runReport(["report", "balance", "--company", company]);
    expect(bs.exitCode).toBe(2);

    cleanupDir(root);
  });

  test("returns ok:false JSON and exit code 1 for an inverted period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reportcli-inverted-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const tb = await runReport(["report", "trial-balance", "--company", company, "--from", "2026-05-31", "--to", "2026-05-01", "--format", "json"]);
    expect(tb.exitCode).toBe(1);
    const parsed = JSON.parse(tb.stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.length).toBeGreaterThan(0);

    cleanupDir(root);
  });
});
