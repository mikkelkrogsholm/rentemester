// Tests: src/cli/journal.ts, src/cli.ts (journal reverse CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("journal reverse CLI", () => {
  test("creates a reversal entry for an existing posted journal resolved by stable match fields", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reversecli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "journal", "reverse", "--company", company, "--match-text", "Software expense booked from vendor invoice", "--match-date", "2026-05-16", "--date", "2026-05-17", "--reason", "Wrong period"], {
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
    expect(parsed.originalEntryId).toBe(1);
  });

  test("fails when match-text is ambiguous without additional narrowing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-reversecli-ambiguous-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "journal", "reverse", "--company", company, "--match-text", "Software expense booked from vendor invoice", "--date", "2026-05-17", "--reason", "Wrong period"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(2);
    expect(stderr).toContain("Multiple journal entries matched --match-text");
  });
});
