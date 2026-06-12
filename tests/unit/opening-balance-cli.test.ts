// Tests: src/cli/opening-balance.ts, src/cli.ts (opening balance post CLI, #179)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  return proc;
}

describe("opening balance post CLI", () => {
  test("posts a balanced primobalance from a JSON payload", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-opening-balance-cli-"));
    const company = join(root, "company");
    const inputFile = join(root, "primo.json");
    writeFileSync(
      inputFile,
      JSON.stringify({
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000, text: "Bankindestående" },
          { accountNo: "5000", creditAmount: 50000, text: "Egenkapital primo" },
        ],
      }),
    );

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = runCli([
      "opening-balance", "post",
      "--company", company,
      "--input", inputFile,
    ]);
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(typeof parsed.entryNo).toBe("string");
  });

  test("rejects a duplicate primobalance with a non-zero exit code", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-opening-balance-cli-dup-"));
    const company = join(root, "company");
    const inputFile = join(root, "primo.json");
    writeFileSync(
      inputFile,
      JSON.stringify({
        cutOverDate: "2026-01-01",
        lines: [
          { accountNo: "2000", debitAmount: 50000 },
          { accountNo: "5000", creditAmount: 50000 },
        ],
      }),
    );

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts opening-balance post --company ${company} --input ${inputFile}`.quiet();

    const proc = runCli([
      "opening-balance", "post",
      "--company", company,
      "--input", inputFile,
    ]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.join(" ").toLowerCase()).toContain("already");
  });
});
