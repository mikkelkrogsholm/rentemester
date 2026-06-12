// Tests: src/cli/invoice.ts (invoice create — guided path, #212),
//        src/core/invoice.ts (computeInvoiceAmounts)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { computeInvoiceAmounts } from "../../src/core/invoice";

import { cleanupDir } from "../helpers/cleanup";
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

describe("computeInvoiceAmounts (#212 — Rentemester does the arithmetic)", () => {
  test("computes line totals, net, VAT and gross from minimal input", () => {
    const result = computeInvoiceAmounts(
      [
        { description: "Bogføring", quantity: 2, unitPriceExVat: 800 },
        { description: "Rådgivning", quantity: 1, unitPriceExVat: 500 },
      ],
      25,
    );
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected ok");
    expect(result.lines.map((l) => l.lineTotalExVat)).toEqual([1600, 500]);
    expect(result.totals).toEqual({
      vatRate: 0.25,
      netAmount: 2100,
      vatAmount: 525,
      grossAmount: 2625,
    });
  });

  test("rounds VAT to ore precision", () => {
    const result = computeInvoiceAmounts(
      [{ description: "Time", quantity: 3, unitPriceExVat: 333.33 }],
      25,
    );
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected ok");
    // 3 * 333.33 = 999.99 net; 25% VAT = 249.9975 -> 250.00; gross 1249.99
    expect(result.totals.netAmount).toBe(999.99);
    expect(result.totals.vatAmount).toBe(250);
    expect(result.totals.grossAmount).toBe(1249.99);
  });

  test("rejects empty lines and invalid quantities", () => {
    const empty = computeInvoiceAmounts([], 25);
    expect(empty.ok).toBe(false);

    const badQty = computeInvoiceAmounts(
      [{ description: "X", quantity: 0, unitPriceExVat: 100 }],
      25,
    );
    expect(badQty.ok).toBe(false);
    if (badQty.ok) throw new Error("expected failure");
    expect(badQty.errors.some((e) => e.includes("quantity"))).toBe(true);
  });
});

describe("invoice create CLI (#212 — guided path)", () => {
  test("a human enters lines, Rentemester computes and issues a valid invoice", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

      const { stdout, stderr, exitCode } = await run([
        "invoice",
        "create",
        "--company",
        company,
        "--issue-date",
        "2026-05-21",
        "--line",
        "Bogføring og momsafstemning|2|800;Rådgivning|1|500",
        "--vat-rate",
        "25",
        "--buyer-name",
        "Kunde A/S",
        "--buyer-address",
        "Købervej 9, 8000 Aarhus C",
        "--seller-name",
        "Rentemester ApS",
        "--seller-address",
        "Testvej 1, 2100 København Ø",
        "--seller-vat",
        "DK12345678",
        "--format",
        "json",
      ]);

      expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(true);
      expect(parsed.invoiceNumber).toBeTruthy();
      expect(parsed.appliedRules).toContain("DK-INVOICE-ISSUE-001");

      // Rentemester computed every amount — the human supplied none of them.
      expect(parsed.computed.lines.map((l: any) => l.lineTotalExVat)).toEqual([1600, 500]);
      expect(parsed.computed.netAmount).toBe(2100);
      expect(parsed.computed.vatAmount).toBe(525);
      expect(parsed.computed.grossAmount).toBe(2625);

      // The issued, immutable snapshot carries the computed totals.
      const stored = JSON.parse(readFileSync(parsed.storedPath, "utf8"));
      expect(stored.totals).toEqual({
        netAmount: 2100,
        vatRate: 0.25,
        vatAmount: 525,
        grossAmount: 2625,
      });
      expect(stored.lines).toEqual([
        { description: "Bogføring og momsafstemning", quantity: 2, unitPriceExVat: 800, lineTotalExVat: 1600 },
        { description: "Rådgivning", quantity: 1, unitPriceExVat: 500, lineTotalExVat: 500 },
      ]);
    } finally {
      cleanupDir(root);
    }
  });

  test("defaults to 25% VAT when --vat-rate is omitted", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const { stdout, exitCode } = await run([
        "invoice",
        "create",
        "--company",
        company,
        "--issue-date",
        "2026-05-21",
        "--line",
        "Konsulentarbejde|1|1000",
        "--buyer-name",
        "Kunde A/S",
        "--buyer-address",
        "Købervej 9, 8000 Aarhus C",
        "--seller-name",
        "Rentemester ApS",
        "--seller-address",
        "Testvej 1, 2100 København Ø",
        "--seller-vat",
        "DK12345678",
        "--format",
        "json",
      ]);
      expect(exitCode).toBe(0);
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(true);
      expect(parsed.computed.vatRatePercent).toBe(25);
      expect(parsed.computed.vatAmount).toBe(250);
      expect(parsed.computed.grossAmount).toBe(1250);
    } finally {
      cleanupDir(root);
    }
  });

  test("rejects a malformed --line with a clear error and exit code 1", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const { stdout, exitCode } = await run([
        "invoice",
        "create",
        "--company",
        company,
        "--issue-date",
        "2026-05-21",
        "--line",
        "Just a description with no numbers",
        "--format",
        "json",
      ]);
      expect(exitCode).toBe(1);
      const parsed = JSON.parse(stdout);
      expect(parsed.ok).toBe(false);
      expect(parsed.errors.join(" ")).toContain("exactly 3 fields");
    } finally {
      cleanupDir(root);
    }
  });

  test("missing --issue-date fails as a usage error (exit code 2)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const { stderr, exitCode } = await run([
        "invoice",
        "create",
        "--company",
        company,
        "--line",
        "Arbejde|1|100",
        "--format",
        "json",
      ]);
      expect(exitCode).toBe(2);
      expect(stderr).toContain("--issue-date");
    } finally {
      cleanupDir(root);
    }
  });
});

describe("invoice create actor allowlist (#265)", () => {
  // #265: `invoice create` issues a real, locked, immutable invoice through the
  // SAME core as `invoice issue`. An unknown --actor must be rejected for
  // `invoice create` EXACTLY as it is for `invoice issue` — otherwise the
  // allowlist is bypassed and an unauthorised party can issue invoices.
  test("an unknown --actor is rejected for invoice create, just like invoice issue", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-create-actor-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

      // Baseline: `invoice issue` correctly rejects the unknown actor.
      const issueRun = await run([
        "invoice",
        "issue",
        "--company",
        company,
        "--input",
        "examples/full-invoice.dk.json",
        "--actor",
        "user:anyone",
      ]);
      expect(issueRun.exitCode).toBe(2);
      expect(issueRun.stderr).toContain("is not in config/policy.yaml actor_allowlist");

      // `invoice create` MUST reject the same unknown actor identically.
      const createRun = await run([
        "invoice",
        "create",
        "--company",
        company,
        "--issue-date",
        "2026-05-21",
        "--line",
        "Arbejde|1|1000",
        "--buyer-name",
        "Kunde A/S",
        "--buyer-address",
        "Købervej 9, 8000 Aarhus C",
        "--seller-name",
        "Rentemester ApS",
        "--seller-address",
        "Testvej 1, 2100 København Ø",
        "--seller-vat",
        "DK12345678",
        "--actor",
        "user:anyone",
        "--format",
        "json",
      ]);
      expect(createRun.exitCode).toBe(2);
      expect(createRun.stderr).toContain("is not in config/policy.yaml actor_allowlist");
      expect(createRun.stderr).toContain("user:anyone");
    } finally {
      cleanupDir(root);
    }
  });
});
