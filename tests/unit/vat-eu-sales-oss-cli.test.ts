// Tests: src/cli/vat.ts (vat eu-sales-list, vat oss-report CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

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

describe("vat eu-sales-list CLI", () => {
  test("lists foreign reverse-charge sales per customer VAT number", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-euslist-cli-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    // VIES must be seeded for the buyer before a foreign reverse-charge invoice issues.
    await Bun.$`bun run scripts/seed-vies-validation.ts ${company} DE123456789`.quiet();

    const invoicePath = join(root, "invoice.json");
    writeFileSync(
      invoicePath,
      JSON.stringify({
        invoiceType: "full",
        vatTreatment: "foreign_reverse_charge",
        reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
        issueDate: "2026-05-10",
        seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
        buyer: { name: "Kunde GmbH", address: "EU-vej 1", vatOrCvr: "DE123456789" },
        lines: [{ description: "Konsulentydelse", quantity: 1, unitPriceExVat: 4000, lineTotalExVat: 4000 }],
        totals: { netAmount: 4000, grossAmount: 4000 },
        currency: "DKK",
      }),
    );
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoicePath}`.quiet();

    const { stdout, stderr, exitCode } = await run([
      "vat", "eu-sales-list", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31",
    ]);
    rmSync(root, { recursive: true, force: true });

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.customers.length).toBe(1);
    expect(parsed.customers[0].vatNumber).toBe("DE123456789");
    expect(parsed.customers[0].totalValue).toBe(4000);
    expect(parsed.totalValue).toBe(4000);
    expect(parsed.appliedRules).toContain("DK-VAT-EU-SALES-LIST-001");
  });
});

describe("vat oss-report CLI", () => {
  test("returns a deterministic OSS skeleton for a period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-ossreport-cli-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const { stdout, stderr, exitCode } = await run([
      "vat", "oss-report", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31",
    ]);
    rmSync(root, { recursive: true, force: true });

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.ossConsumerSalesBase).toBe(0);
    expect(parsed.submission).toBe(false);
    expect(parsed.appliedRules).toContain("DK-VAT-OSS-001");
  });
});
