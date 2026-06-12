// Tests: src/cli/invoice.ts, src/cli.ts (invoice payments CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice payment CLI", () => {
  test("applies payment to issued invoice from input json", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoicepay-cli-"));
    const company = join(root, "company");
    const paymentJson = join(root, "payment.json");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    writeFileSync(paymentJson, JSON.stringify({
      invoiceDocumentId: 1,
      paymentDate: "2026-05-20",
      amount: 1250,
      note: "Paid in full"
    }, null, 2));

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "apply-payment", "--company", company, "--input", paymentJson], {
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
    expect(parsed.openBalance).toBe(0);
  });
});
