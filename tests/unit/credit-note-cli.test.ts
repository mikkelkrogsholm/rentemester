// Tests: src/cli/invoice.ts, src/cli.ts (credit-note CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("credit note CLI", () => {
  test("issues a credit note for an issued invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-credit-cli-"));
    const company = join(root, "company");
    const input = join(root, "credit-note.json");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    writeFileSync(input, JSON.stringify({
      originalInvoiceDocumentId: 1,
      issueDate: "2026-05-17",
      reason: "Invoice issued in error"
    }, null, 2));

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "credit-note", "--company", company, "--input", input], {
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
    expect(parsed.appliedRules).toContain("DK-CREDIT-NOTE-001");
  });
});
