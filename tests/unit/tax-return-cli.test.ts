// Tests: src/cli/tax.ts, src/cli.ts (corporate tax return / oplysningsskema CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("report tax CLI", () => {
  test("emits an oplysningsskema preparation for a locked fiscal year", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-taxcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company} --cvr DK12345678`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();
    await Bun.$`bun run src/cli.ts journal post --company ${company} --input examples/journal-entry.expense.json`.quiet();
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-01-01 --to 2026-12-31 --kind fiscal_year --status closed`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "report", "tax", "--company", company, "--from", "2026-01-01", "--to", "2026-12-31"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.fiscalYearStart).toBe("2026-01-01");
    expect(parsed.corporateTaxRate).toBe(0.22);
    // The expense-only journal posting yields a loss; the loss carry-forward is
    // a needs-review item, never silently folded into a computed tax.
    expect(parsed.needsReview.some((r: { kind: string }) => r.kind === "tax_loss_carry_forward")).toBe(true);
    // `init` leaves the company form unset (it is only filled by `company
    // sync-cvr`, a network call); the slice flags that as needs-review rather
    // than guessing the corporate-tax treatment.
    expect(parsed.needsReview.some((r: { kind: string }) => r.kind === "company_form_out_of_scope")).toBe(true);
    expect(parsed.corporateTax).toBeNull();
  });

  test("fails clearly when the fiscal year is not locked", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-taxcli-open-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "report", "tax", "--company", company, "--from", "2026-01-01", "--to", "2026-12-31"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    rmSync(root, { recursive: true, force: true });
    const parsed = JSON.parse(stdout);
    expect(exitCode).toBe(1);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.length).toBeGreaterThan(0);
  });
});
