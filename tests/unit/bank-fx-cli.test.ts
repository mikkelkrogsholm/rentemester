// Tests: src/cli/bank.ts, src/cli.ts (bank FX CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("bank FX import CLI", () => {
  test("imports a non-DKK CSV row when FX metadata is present", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-bankfxcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "bank", "import", "--company", company, "--file", "examples/bank-transactions-eur.csv"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.imported).toBe(1);
  });
});
