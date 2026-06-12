// Tests: src/cli/email.ts, src/cli.ts (invoice send CLI, #180)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
const INVOICE = {
  invoiceType: "full",
  vatTreatment: "standard",
  issueDate: "2026-05-16",
  dueDate: "2026-06-15",
  invoiceNumber: "2026-0001",
  seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
  lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
  totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
  currency: "DKK",
};

const SMTP_CONFIG = {
  host: "smtp.example.test",
  port: 587,
  fromAddress: "faktura@rentemester.test",
  fromName: "Rentemester ApS",
  dryRun: true,
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

describe("invoice send CLI", () => {
  test("sends an issued invoice via the dry-run transport and records the send log", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-cli-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice.json");
    writeFileSync(invoiceInput, JSON.stringify(INVOICE, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();

    // SMTP config lives in config/smtp.json — never in the ledger DB.
    writeFileSync(join(company, "config", "smtp.json"), JSON.stringify(SMTP_CONFIG, null, 2));

    const result = await run([
      "invoice",
      "send",
      "--company",
      company,
      "--invoice-number",
      "2026-0001",
      "--to",
      "kunde@example.test",
    ]);

    cleanupDir(root);
    expect({ exitCode: result.exitCode, stderr: result.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(result.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.recipient).toBe("kunde@example.test");
    expect(parsed.kind).toBe("invoice");
    expect(parsed.messageId).toBeDefined();
  });

  test("fails clearly when the SMTP config file is missing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-cli-noconfig-"));
    const company = join(root, "company");
    const invoiceInput = join(root, "invoice.json");
    writeFileSync(invoiceInput, JSON.stringify(INVOICE, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoiceInput}`.quiet();

    const result = await run([
      "invoice",
      "send",
      "--company",
      company,
      "--invoice-number",
      "2026-0001",
      "--to",
      "kunde@example.test",
    ]);

    cleanupDir(root);
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr + result.stdout).toContain("smtp");
  });
});
