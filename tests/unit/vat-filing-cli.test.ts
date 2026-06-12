// Tests: src/cli/vat.ts, src/cli.ts (VAT filing / momsangivelse CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("vat momsangivelse CLI", () => {
  test("emits a filing-ready momsangivelse for a closed VAT period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vatfilingcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --status closed`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "vat", "momsangivelse", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.periodStatus).toBe("closed");
    expect(parsed.rubrikker.kobsmoms).toBe(250);
    expect(parsed.rubrikker.salgsmoms).toBe(0);
    expect(parsed.rubrikker.momstilsvar).toBe(-250);
  });

  test("fails clearly when the VAT period is still open", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vatfilingcli-open-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "vat", "filing", "--company", company, "--from", "2026-05-01", "--to", "2026-05-31"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    const parsed = JSON.parse(stdout);
    expect(exitCode).toBe(1);
    expect(parsed.ok).toBe(false);
    expect(parsed.periodStatus).toBe("open");
    expect(parsed.errors.length).toBeGreaterThan(0);
  });
});
