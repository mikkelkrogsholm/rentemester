// Tests: src/cli/mileage.ts, src/cli.ts (mileage CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";

import { cleanupDir } from "../helpers/cleanup";
async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, USER: "tester", RENTEMESTER_ACTOR: "" },
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const company = join(root, "company");
  const db = openDb(ensureCompanyDirs(company).db);
  migrate(db);
  db.run(
    `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`,
  );
  db.close();
  return { root, company };
}

describe("mileage CLI", () => {
  test("creates, lists and reports mileage entries deterministically", async () => {
    const { root, company } = freshCompany("rentemester-mileage-cli-");

    const create = await runCli([
      "mileage", "log", "--company", company,
      "--date", "2026-03-10",
      "--purpose", "Kundemøde i Aarhus",
      "--from", "København",
      "--to", "Aarhus",
      "--km", "312.5",
      "--vehicle", "AB 12 345",
      "--driver", "Mikkel Krogsholm",
      "--rate-per-km", "3.79",
      "--rate-basis", "Statens takst 2026 - bekraeftet af bruger",
      "--rate-source", "https://skat.dk/satser",
    ]);
    expect({ exitCode: create.exitCode, stderr: create.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const created = JSON.parse(create.stdout);
    expect(created.ok).toBe(true);
    expect(created.entryNo).toBe("MIL-2026-000001");

    const list = await runCli(["mileage", "list", "--company", company]);
    expect(list.exitCode).toBe(0);
    const listed = JSON.parse(list.stdout);
    expect(listed.ok).toBe(true);
    expect(listed.count).toBe(1);
    expect(listed.rows[0].amountBasis).toBe(1184.38);

    const report = await runCli([
      "mileage", "report", "--company", company, "--from", "2026-03-01", "--to", "2026-03-31",
    ]);
    expect(report.exitCode).toBe(0);
    const reported = JSON.parse(report.stdout);
    expect(reported.ok).toBe(true);
    expect(reported.totalKilometers).toBe(312.5);
    expect(reported.totalAmountBasis).toBe(1184.38);

    cleanupDir(root);
  });

  test("rejects a mileage log call with a missing required field", async () => {
    const { root, company } = freshCompany("rentemester-mileage-cli-bad-");

    const create = await runCli([
      "mileage", "log", "--company", company,
      "--date", "2026-03-10",
      "--purpose", "Kundemøde",
      "--from", "København",
      "--to", "Aarhus",
      "--km", "100",
      "--vehicle", "AB 12 345",
      "--driver", "Mikkel",
      "--rate-per-km", "3.79",
    ]);
    // Missing --rate-basis: deterministic non-zero exit + ok:false.
    expect(create.exitCode).toBe(1);
    const parsed = JSON.parse(create.stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors).toContain("rateBasis is required (user-supplied / source-backed)");

    cleanupDir(root);
  });
});
