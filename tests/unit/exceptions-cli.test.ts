// Tests: src/cli/exceptions.ts, src/cli.ts (exceptions CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("exceptions CLI", () => {
  test("lists and resolves unmatched-bank exceptions created during bank import", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exceptions-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const imported = Bun.spawn(["bun", "run", "src/cli.ts", "bank", "import", "--company", company, "--file", "examples/bank-transactions.csv", "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const importedStdout = await new Response(imported.stdout).text();
    const importedExitCode = await imported.exited;
    expect(importedExitCode).toBe(0);
    expect(JSON.parse(importedStdout).exceptionsCreated).toBe(2);

    const listed = Bun.spawn(["bun", "run", "src/cli.ts", "exceptions", "list", "--company", company, "--status", "open", "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const listedStdout = await new Response(listed.stdout).text();
    const listedExitCode = await listed.exited;
    expect(listedExitCode).toBe(0);
    const listedParsed = JSON.parse(listedStdout);
    expect(listedParsed.count).toBe(2);
    expect(listedParsed.rows.every((row: any) => row.type === "UNMATCHED_BANK_TRANSACTION")).toBe(true);

    const exceptionId = listedParsed.rows[0].id;
    const resolved = Bun.spawn(["bun", "run", "src/cli.ts", "exceptions", "resolve", "--company", company, "--id", String(exceptionId), "--note", "Handled manually", "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const resolvedStdout = await new Response(resolved.stdout).text();
    const resolvedExitCode = await resolved.exited;
    expect(resolvedExitCode).toBe(0);
    expect(JSON.parse(resolvedStdout).resolved).toBe(true);

    const resolvedList = Bun.spawn(["bun", "run", "src/cli.ts", "exceptions", "list", "--company", company, "--status", "resolved", "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const resolvedListStdout = await new Response(resolvedList.stdout).text();
    const resolvedListExitCode = await resolvedList.exited;
    expect(resolvedListExitCode).toBe(0);
    const resolvedParsed = JSON.parse(resolvedListStdout);
    expect(resolvedParsed.count).toBe(1);
    expect(resolvedParsed.rows[0].resolutionNote).toBe("Handled manually");

    cleanupDir(root);
  });

  test("records a blocked document-ingest exception when metadata validation fails", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exceptions-doccli-"));
    const company = join(root, "company");
    const file = join(root, "invoice.txt");
    const metadata = join(root, "metadata.json");

    writeFileSync(file, "Broken invoice\n1250 DKK\n");
    writeFileSync(metadata, JSON.stringify({
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-ERR-1",
      amountIncVat: 1250,
      currency: "DKK"
    }, null, 2));

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const failed = Bun.spawn(["bun", "run", "src/cli.ts", "documents", "ingest", "--company", company, "--file", file, "--metadata", metadata, "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const failedStdout = await new Response(failed.stdout).text();
    const failedExitCode = await failed.exited;
    expect(failedExitCode).toBe(1);
    expect(JSON.parse(failedStdout).errors.some((entry: string) => entry.includes("sender.name is required"))).toBe(true);

    const listed = Bun.spawn(["bun", "run", "src/cli.ts", "exceptions", "list", "--company", company, "--status", "open", "--format", "json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const listedStdout = await new Response(listed.stdout).text();
    const listedExitCode = await listed.exited;
    expect(listedExitCode).toBe(0);
    const parsed = JSON.parse(listedStdout);
    const blocked = parsed.rows.find((row: any) => row.type === "DOCUMENT_INGEST_BLOCKED");
    expect(blocked).toBeTruthy();
    expect(blocked.sourceEvidence.file).toBe(file);
    expect(blocked.sourceEvidence.errors.some((entry: string) => entry.includes("sender.name is required"))).toBe(true);

    cleanupDir(root);
  });
});
