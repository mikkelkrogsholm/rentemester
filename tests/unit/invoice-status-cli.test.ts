// Tests: src/cli/invoice.ts, src/cli.ts (invoice status CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("invoice status CLI", () => {
  test("shows overdue classification at a chosen as-of date", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "status", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-20"], {
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
    expect(parsed.effectiveDueDate).toBe("2026-06-15");
    expect(parsed.isOverdue).toBe(true);
    expect(parsed.overdueDays).toBe(5);
  });

  // #230: a supplied --invoice-number that resolves to nothing is a BUSINESS
  // error (exit 1, ok:false) — not a usage error. Retrying the same call is
  // pointless, so it must not exit 2.
  test("an unresolved --invoice-number is a business error (exit 1), not a usage error", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-missing-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "invoice", "status", "--company", company, "--invoice-number", "9999-9999"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.join(" ")).toContain("9999-9999");
    expect(parsed.errors.join(" ")).not.toContain("Missing required");
  });

  // #230: omitting both --document-id and --invoice-number is still a usage
  // error (exit 2 — fix the call).
  test("omitting all identifying flags is a usage error (exit 2)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-noflag-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "invoice", "status", "--company", company],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(2);
    expect(stderr).toContain("Missing required --document-id");
  });

  // #230: `invoice post` shares the same resolver — an unresolved number must
  // not print the misleading "missing flag" error.
  test("invoice post with an unresolved --invoice-number exits 1, not 2", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-post-missing-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "invoice", "post", "--company", company, "--invoice-number", "9999-9999"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(1);
    expect(stderr).not.toContain("Missing required");
    expect(JSON.parse(stdout).ok).toBe(false);
  });

  // #233: `invoice status` in human mode must carry one decisive
  // settled/outstanding summary line.
  test("human-mode status carries a single settled/outstanding summary line", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-invoice-status-summary-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "invoice", "status", "--company", company, "--invoice-number", "2026-0001", "--as-of", "2026-06-01", "--format", "human"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    cleanupDir(root);
    expect(exitCode).toBe(0);
    // An unpaid invoice: the summary states the customer still owes the amount.
    expect(stdout).toContain("→ Udestående i alt:");
    expect(stdout).toContain("kunden skylder stadig");
  });
});
