// Tests: src/cli/recurring-invoice.ts, src/cli.ts (recurring-invoice CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
const TEMPLATE_INPUT = {
  name: "Monthly retainer",
  interval: "monthly",
  firstIssueDate: "2026-01-15",
  paymentTermsDays: 30,
  deliveryPeriodMode: "issue_month",
  invoice: {
    invoiceType: "full",
    vatTreatment: "standard",
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9" },
    lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
  },
};

async function run(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("recurring-invoice CLI", () => {
  test("creates a template, generates an invoice, and is idempotent on rerun", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-recurring-cli-"));
    const company = join(root, "company");
    const inputPath = join(root, "template.json");
    writeFileSync(inputPath, JSON.stringify(TEMPLATE_INPUT), "utf8");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const created = await run([
      "recurring-invoice",
      "create",
      "--company",
      company,
      "--input",
      inputPath,
    ]);
    expect({ exitCode: created.exitCode, stderr: created.stderr }).toEqual({
      exitCode: 0,
      stderr: "",
    });
    const createdJson = JSON.parse(created.stdout);
    expect(createdJson.ok).toBe(true);
    expect(createdJson.templateId).toBeGreaterThan(0);

    const generated = await run([
      "recurring-invoice",
      "generate",
      "--company",
      company,
      "--template-id",
      String(createdJson.templateId),
      "--as-of",
      "2026-01-20",
    ]);
    expect({ exitCode: generated.exitCode, stderr: generated.stderr }).toEqual({
      exitCode: 0,
      stderr: "",
    });
    const generatedJson = JSON.parse(generated.stdout);
    expect(generatedJson.ok).toBe(true);
    expect(generatedJson.created).toBe(true);
    expect(generatedJson.invoiceNumber).toBe("2026-0001");
    expect(generatedJson.appliedRules).toContain("DK-RECURRING-INVOICE-GENERATE-001");

    const rerun = await run([
      "recurring-invoice",
      "generate",
      "--company",
      company,
      "--template-id",
      String(createdJson.templateId),
      "--as-of",
      "2026-01-20",
    ]);
    const rerunJson = JSON.parse(rerun.stdout);
    expect(rerunJson.ok).toBe(true);
    expect(rerunJson.created).toBe(false);
    expect(rerunJson.invoiceNumber).toBe("2026-0001");

    const listed = await run([
      "recurring-invoice",
      "list",
      "--company",
      company,
      "--format",
      "json",
    ]);
    const listedJson = JSON.parse(listed.stdout);
    expect(listedJson.ok).toBe(true);
    expect(listedJson.count).toBe(1);

    cleanupDir(root);
  });
});
