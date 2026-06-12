// Tests: src/cli/documents.ts, src/cli.ts (document CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("documents ingest CLI", () => {
  test("returns exit code 0 for valid foreign-currency cash-register receipt metadata", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-doccli-cash-"));
    const company = join(root, "company");
    const file = join(root, "receipt.txt");
    const metadata = join(root, "metadata-cash.json");

    writeFileSync(file, "Coffee receipt\n12.00 EUR\n");
    writeFileSync(metadata, JSON.stringify({
      source: "photo-upload",
      documentType: "cash_register_receipt",
      currency: "EUR"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", file, "--metadata", metadata], {
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
    expect(parsed.documentNo).toContain("DOC-");
  });

  test("returns exit code 0 for valid foreign physical-only receipt metadata", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-doccli-foreign-"));
    const company = join(root, "company");
    const file = join(root, "ticket.txt");
    const metadata = join(root, "metadata-foreign.json");

    writeFileSync(file, "Metro ticket\n8.50 EUR\n");
    writeFileSync(metadata, JSON.stringify({
      source: "mobile-scan",
      currency: "EUR",
      exemptionCode: "FOREIGN_PHYSICAL_ONLY"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", file, "--metadata", metadata], {
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
    expect(parsed.documentNo).toContain("DOC-");
  });

  test("accepts purchase/sale metadata without payment details", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-doccli-"));
    const company = join(root, "company");
    const file = join(root, "invoice.txt");
    const metadata = join(root, "metadata.json");

    writeFileSync(file, "Invoice 1002\n1250 DKK\n");
    writeFileSync(metadata, JSON.stringify({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1002",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", file, "--metadata", metadata], {
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
    expect(parsed.documentNo).toContain("DOC-");
  });

  test("blocks duplicate logical supplier invoices unless --force is used", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-doccli-force-"));
    const company = join(root, "company");
    const fileA = join(root, "invoice-a.txt");
    const fileB = join(root, "invoice-b.txt");
    const metadata = join(root, "metadata.json");

    writeFileSync(fileA, "Invoice INV-1003\n1250 DKK\n");
    writeFileSync(fileB, "Invoice INV-1003\n1250 DKK\nrescanned\n");
    writeFileSync(metadata, JSON.stringify({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1003",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      vatAmount: 250
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file ${fileA} --metadata ${metadata}`.quiet();

    const blocked = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", fileB, "--metadata", metadata], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const blockedStdout = await new Response(blocked.stdout).text();
    const blockedExitCode = await blocked.exited;

    const forced = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", fileB, "--metadata", metadata, "--force"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const forcedStdout = await new Response(forced.stdout).text();
    const forcedExitCode = await forced.exited;

    cleanupDir(root);
    expect(blockedExitCode).toBe(1);
    expect(JSON.parse(blockedStdout).errors[0]).toContain("Use --force to add another scan");
    expect(forcedExitCode).toBe(0);
    expect(JSON.parse(forcedStdout).ok).toBe(true);
  });
});
