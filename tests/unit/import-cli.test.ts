// Tests: src/cli/import.ts, src/cli.ts (import framework CLI, #185)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync, copyFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
function runCli(args: string[]) {
  return Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
}

const SAMPLE = join(process.cwd(), "examples/import-synthetic.csv");

describe("import run CLI", () => {
  test("imports the synthetic sample and lands a primobalance", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-import-cli-"));
    const company = join(root, "company");
    const file = join(root, "export.csv");
    copyFileSync(SAMPLE, file);

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = runCli([
      "import", "run",
      "--company", company,
      "--file", file,
      "--system", "synthetic-csv",
    ]);
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.sourceSystem).toBe("synthetic-csv");
    expect(typeof parsed.entryNo).toBe("string");
    expect(Array.isArray(parsed.auditTrail)).toBe(true);
  });

  test("rejects an unbalanced export with a non-zero exit code", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-import-cli-unbal-"));
    const company = join(root, "company");
    const file = join(root, "export.csv");
    writeFileSync(
      file,
      [
        "# source: synthetic-csv",
        "# cutOverDate: 2026-01-01",
        "section,accountNo,name,debit,credit",
        "account,2000,Bank,,",
        "account,5000,Egenkapital,,",
        "opening,2000,,80000,",
        "opening,5000,,,70000",
      ].join("\n"),
    );

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = runCli([
      "import", "run",
      "--company", company,
      "--file", file,
    ]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.join(" ").toLowerCase()).toContain("balance");
  });

  test("lists the available import systems", async () => {
    const proc = runCli(["import", "systems", "--format", "json"]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.systems.some((s: { system: string }) => s.system === "synthetic-csv")).toBe(true);
    expect(parsed.systems.some((s: { system: string }) => s.system === "dinero")).toBe(true);
  });

  test("imports a Dinero export directory: reconciles chart & posts the primobalance", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-import-cli-dinero-"));
    const company = join(root, "company");
    const fixture = join(process.cwd(), "examples/import-dinero");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = runCli([
      "import", "run",
      "--company", company,
      "--file", fixture,
      "--system", "dinero",
    ]);
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.sourceSystem).toBe("dinero");
    // The fixture carries a Posteringer.csv, so the import reconciles the chart
    // AND posts the cut-over year's primobalance (#194) end-to-end.
    expect(parsed.entryNo).toBeTruthy();
    expect(parsed.cutOverDate).toBe("2025-01-01");
    expect(parsed.openingBalanceLineCount).toBe(8);
    expect(parsed.chart.created.length).toBeGreaterThan(0);
    expect(parsed.company.updatedFields).toContain("cvr");
  });
});
