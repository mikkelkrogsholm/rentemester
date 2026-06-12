// Tests: src/cli/invoice.ts, src/cli.ts (invoice list CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
function invoicePayload(overrides: Record<string, unknown> = {}) {
  return {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    invoiceNumber: "2026-0001",
    seller: {
      name: "Rentemester ApS",
      address: "Testvej 1, 2100 København Ø",
      vatOrCvr: "DK12345678",
    },
    buyer: {
      name: "Kunde A/S",
      address: "Købervej 9, 8000 Aarhus C",
      vatOrCvr: "DK87654321",
    },
    lines: [
      {
        description: "Bogføring og momsafstemning",
        quantity: 1,
        unitPriceExVat: 1000,
        lineTotalExVat: 1000,
      },
    ],
    totals: {
      netAmount: 1000,
      vatRate: 0.25,
      vatAmount: 250,
      grossAmount: 1250,
    },
    currency: "DKK",
    dueDate: "2026-06-15",
    ...overrides,
  };
}

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

describe("invoice list/find/overdue CLI", () => {
  test("lists open invoices and finds invoices by customer", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-list-"));
    const company = join(root, "company");
    const invoice1 = join(root, "invoice-1.json");
    const invoice2 = join(root, "invoice-2.json");
    const payment2 = join(root, "payment-2.json");

    writeFileSync(invoice1, JSON.stringify(invoicePayload(), null, 2));
    writeFileSync(invoice2, JSON.stringify(invoicePayload({
      invoiceNumber: "2026-0002",
      issueDate: "2026-05-10",
      dueDate: "2026-05-25",
      buyer: {
        name: "Beta ApS",
        address: "Beta Allé 2, 5000 Odense C",
        vatOrCvr: "DK11223344",
      },
      lines: [{ description: "Retainer", quantity: 1, unitPriceExVat: 2000, lineTotalExVat: 2000 }],
      totals: { netAmount: 2000, vatRate: 0.25, vatAmount: 500, grossAmount: 2500 },
    }), null, 2));
    writeFileSync(payment2, JSON.stringify({ invoiceNumber: "2026-0002", paymentDate: "2026-05-26", amount: 2500, note: "Paid" }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoice1}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoice2}`.quiet();
    await Bun.$`bun run src/cli.ts invoice apply-payment --company ${company} --input ${payment2}`.quiet();

    const listed = await runCli(["invoice", "list", "--company", company, "--status", "open", "--as-of", "2026-06-20"]);
    const found = await runCli(["invoice", "find", "--company", company, "--customer", "kunde", "--as-of", "2026-06-20"]);

    cleanupDir(root);

    expect(listed.exitCode).toBe(0);
    expect(listed.stderr).toBe("");
    const listedJson = JSON.parse(listed.stdout);
    expect(listedJson.count).toBe(1);
    expect(listedJson.rows).toHaveLength(1);
    expect(listedJson.rows[0]).toMatchObject({
      invoiceNumber: "2026-0001",
      customerName: "Kunde A/S",
      status: "open",
      isOverdue: true,
      overdueDays: 5,
    });

    expect(found.exitCode).toBe(0);
    expect(found.stderr).toBe("");
    const foundJson = JSON.parse(found.stdout);
    expect(foundJson.count).toBe(1);
    expect(foundJson.rows[0]).toMatchObject({ invoiceNumber: "2026-0001", customerName: "Kunde A/S" });
  });

  test("shows overdue invoices with min-days filter in human output", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-overdue-"));
    const company = join(root, "company");
    const invoice1 = join(root, "invoice-1.json");
    const invoice3 = join(root, "invoice-3.json");

    writeFileSync(invoice1, JSON.stringify(invoicePayload(), null, 2));
    writeFileSync(invoice3, JSON.stringify(invoicePayload({
      invoiceNumber: "2026-0002",
      issueDate: "2026-05-19",
      dueDate: "2026-06-18",
      buyer: {
        name: "Gamma ApS",
        address: "Gamma Gade 3, 9000 Aalborg",
        vatOrCvr: "DK44332211",
      },
      lines: [{ description: "Workshop", quantity: 1, unitPriceExVat: 400, lineTotalExVat: 400 }],
      totals: { netAmount: 400, vatRate: 0.25, vatAmount: 100, grossAmount: 500 },
    }), null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoice1}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input ${invoice3}`.quiet();

    const proc = await runCli(["invoice", "overdue", "--company", company, "--as-of", "2026-06-20", "--min-days", "5", "--format", "human"]);

    cleanupDir(root);

    expect(proc.exitCode).toBe(0);
    expect(proc.stderr).toBe("");
    // Human output is rendered in Danish with kroner-og-øre (#211).
    expect(proc.stdout).toContain("Forfaldne fakturaer pr. 2026-06-20 (1)");
    expect(proc.stdout).toContain("2026-0001");
    expect(proc.stdout).toContain("1.250,00 kr.");
    expect(proc.stdout).toContain("5 dage forfalden");
    expect(proc.stdout).not.toContain("2026-0002");
  });
});
