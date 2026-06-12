// Tests: src/cli/journal.ts, src/cli/bank.ts, src/cli-format.ts,
// src/core/invoice-booking.ts (CLI output-rendering bugs #285, #286, #288)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
async function runCli(args: string[]) {
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

describe("#285 — journal list shows entry amounts", () => {
  test("human journal list shows the entry debit total, not an em dash", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-amount-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();

    const listed = await runCli(["journal", "list", "--company", company, "--format", "human"]);

    cleanupDir(root);

    expect({ exitCode: listed.exitCode, stderr: listed.stderr }).toEqual({
      exitCode: 0,
      stderr: "",
    });
    // Gross of full-invoice.dk.json is 1.250,00 kr; the entry debit total
    // equals the receivable line. The header amount_dkk is null for DKK
    // entries, so the renderer must fall back to the posting sum.
    expect(listed.stdout).toContain("Beløb: 1.250,00 kr.");
    expect(listed.stdout).not.toContain("Beløb: —");
  });
});

describe("#286 — Danish text on journal + bank surfaces", () => {
  test("new issued-invoice journal entries carry Danish posting text", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-journal-da-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();

    const listed = await runCli(["journal", "list", "--company", company, "--format", "json"]);

    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    const rows = JSON.parse(listed.stdout);
    const entry = rows.find((r: any) => r.document_id != null) ?? rows[0];
    expect(entry.text).not.toContain("Issued invoice");
    expect(entry.text).toContain("Faktura 2026-0001");
  });

  test("bank suggest-matches human renderer is Danish", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-suggest-da-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();

    const listed = await runCli(["bank", "suggest-matches", "--company", company, "--format", "human"]);

    cleanupDir(root);

    expect({ exitCode: listed.exitCode, stderr: listed.stderr }).toEqual({
      exitCode: 0,
      stderr: "",
    });
    expect(listed.stdout).not.toContain("Bank transaction");
    expect(listed.stdout).not.toContain("No deterministic suggestions");
    expect(listed.stdout).not.toContain("No unmatched bank transactions");
    expect(listed.stdout).toContain("Banktransaktion");
    expect(listed.stdout).toContain("Ingen sikre forslag");
  });
});

describe("#288 — reconcile bank separates inflows and outflows", () => {
  test("human reconcile report shows inflow and outflow totals separately", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reconcile-split-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    // examples/bank-transactions.csv: one -1.250 expense, one +2.500 deposit,
    // both unreconciled. A single netted figure would read 1.250,00 kr.
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();

    const listed = await runCli([
      "reconcile", "bank", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31", "--format", "human",
    ]);

    cleanupDir(root);

    expect({ exitCode: listed.exitCode, stderr: listed.stderr }).toEqual({
      exitCode: 0,
      stderr: "",
    });
    // The netted total (1.250,00 kr.) must not be the headline figure.
    expect(listed.stdout).not.toContain("Uafstemt beløb i alt:");
    expect(listed.stdout).toContain("Uafstemte indbetalinger");
    expect(listed.stdout).toContain("Uafstemte udbetalinger");
    expect(listed.stdout).toContain("2.500,00 kr.");
    expect(listed.stdout).toContain("1.250,00 kr.");
  });

  test("reconcile bank --format json output keeps the netted total field", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reconcile-json-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts bank import --company ${company} --file examples/bank-transactions.csv`.quiet();

    const listed = await runCli([
      "reconcile", "bank", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31", "--format", "json",
    ]);

    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    const parsed = JSON.parse(listed.stdout);
    expect(parsed.ok).toBe(true);
    // JSON contract must stay byte-identical: the existing field is preserved.
    expect(parsed.unmatchedAmountTotal).toBe(1250);
  });
});
