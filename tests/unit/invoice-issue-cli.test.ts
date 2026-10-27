// Tests: src/cli/invoice.ts, src/cli.ts (invoice issue CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("invoice issue CLI", () => {
  test("preview does not reserve a number and incomplete PDF approval is rejected", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-approved-cli-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
      const invoke = async (args: string[]) => {
        const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
        const stdout = await new Response(proc.stdout).text();
        return { exitCode: await proc.exited, result: JSON.parse(stdout) };
      };
      const preview = await invoke(["invoice", "preview-number", "--company", company, "--issue-date", "2026-05-16"]);
      expect(preview.result).toMatchObject({ ok: true, invoiceNumber: "2026-0001", reserved: false });
      const rejected = await invoke(["invoice", "issue", "--company", company, "--input", "examples/full-invoice.dk.json", "--approved-pdf", "example.pdf"]);
      expect(rejected.exitCode).toBe(1);
      expect(rejected.result.ok).toBe(false);
      expect((await invoke(["invoice", "preview-number", "--company", company, "--issue-date", "2026-05-16"])).result.invoiceNumber).toBe(preview.result.invoiceNumber);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
  test("issues an invoice from input json", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-issue-cli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "invoice", "issue", "--company", company, "--input", "examples/full-invoice.dk.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.invoiceNumber).toBeTruthy();
    expect(parsed.appliedRules).toContain("DK-INVOICE-ISSUE-001");
  });
});
