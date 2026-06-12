// Tests: src/cli/vat.ts, src/cli.ts (VAT representation CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("representation VAT CLI", () => {
  test("posts a limited-deduction representation purchase from input json", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-repr-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "vat", "post-representation-purchase", "--company", company, "--input", "examples/representation-purchase.json"], {
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
    expect(parsed.appliedRules).toContain("DK-VAT-REPRESENTATION-001");
  });
});
