// Tests: src/cli/annual-report.ts, src/cli.ts (annual report CLI, #177)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function run(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { exitCode, stdout, stderr };
}

// Spins up a company with CVR, posts a balanced year and locks it.
async function preparedCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const company = join(root, "company");
  await Bun.$`bun run src/cli.ts init --company ${company} --cvr DK12345678`.quiet();

  // Income lines require a source document; ingest the example invoice so a
  // document with id 1 exists to attach the revenue entry to.
  await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/vendor-invoice.txt --metadata examples/vendor-invoice.metadata.json`.quiet();

  const journalDir = mkdtempSync(join(tmpdir(), `${prefix}j-`));
  const open = join(journalDir, "open.json");
  const sale = join(journalDir, "sale.json");
  Bun.write(
    open,
    JSON.stringify({
      transactionDate: "2025-01-02",
      text: "Indskud",
      lines: [
        { accountNo: "2000", debitAmount: 50000 },
        { accountNo: "5000", creditAmount: 50000 },
      ],
    }),
  );
  Bun.write(
    sale,
    JSON.stringify({
      transactionDate: "2025-06-15",
      text: "Konsulentsalg",
      documentId: 1,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    }),
  );
  await Bun.$`bun run src/cli.ts journal post --company ${company} --input ${open}`.quiet();
  await Bun.$`bun run src/cli.ts journal post --company ${company} --input ${sale}`.quiet();
  await Bun.$`bun run src/cli.ts period close --company ${company} --from 2025-01-01 --to 2025-12-31 --kind fiscal_year`.quiet();
  return { root, company };
}

describe("report annual CLI", () => {
  test("emits the arsrapport JSON for a locked fiscal year", async () => {
    const { root, company } = await preparedCompany("rentemester-annualcli-");

    const res = await run(["report", "annual", "--company", company, "--from", "2025-01-01", "--to", "2025-12-31"]);
    expect({ exitCode: res.exitCode, stderr: res.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(res.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.regnskabsklasse).toBe("B");
    expect(parsed.aretsResultat).toBe(1000);
    expect(parsed.company.cvr).toBe("DK12345678");

    rmSync(root, { recursive: true, force: true });
  });

  test("writes a deterministic iXBRL file with --ixbrl-out", async () => {
    const { root, company } = await preparedCompany("rentemester-annualcli-ix-");
    const out1 = join(root, "arsrapport-1.xhtml");
    const out2 = join(root, "arsrapport-2.xhtml");

    const r1 = await run(["report", "annual", "--company", company, "--from", "2025-01-01", "--to", "2025-12-31", "--ixbrl-out", out1]);
    expect({ exitCode: r1.exitCode, stderr: r1.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const r2 = await run(["report", "annual", "--company", company, "--from", "2025-01-01", "--to", "2025-12-31", "--ixbrl-out", out2]);
    expect(r2.exitCode).toBe(0);

    const xhtml1 = readFileSync(out1, "utf8");
    const xhtml2 = readFileSync(out2, "utf8");
    expect(xhtml1).toBe(xhtml2);
    expect(xhtml1).toContain("xmlns:ix=");

    const parsed = JSON.parse(r1.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.ixbrl.path).toBe(out1);
    expect(typeof parsed.ixbrl.sha256).toBe("string");
    // #177 expansion: the result surfaces the versioned taxonomy subset.
    expect(typeof parsed.ixbrl.taxonomy.name).toBe("string");
    expect(parsed.ixbrl.taxonomy.version).toMatch(/^\d+\.\d+\.\d+$/);

    rmSync(root, { recursive: true, force: true });
  });

  test("prints the bounded iXBRL taxonomy subset with --ixbrl-taxonomy", async () => {
    // Introspection mode: no ledger access, no --from/--to/--company required.
    const res = await run(["report", "annual", "--ixbrl-taxonomy", "--format", "json"]);
    expect({ exitCode: res.exitCode, stderr: res.stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(res.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(parsed.elementCount).toBe(parsed.elements.length);
    expect(parsed.elementCount).toBeGreaterThan(0);
    // The four class-B sections are all represented.
    const sections = new Set(parsed.elements.map((e: { section: string }) => e.section));
    for (const required of ["income-statement", "balance-sheet", "management-statement", "accounting-policies"]) {
      expect(sections.has(required)).toBe(true);
    }
  });

  test("fails with exit code 2 when a required flag is missing", async () => {
    const { root, company } = await preparedCompany("rentemester-annualcli-missing-");
    const res = await run(["report", "annual", "--company", company, "--from", "2025-01-01"]);
    expect(res.exitCode).toBe(2);
    rmSync(root, { recursive: true, force: true });
  });

  test("returns ok:false JSON and exit code 1 when the year is not locked", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-annualcli-open-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company} --cvr DK12345678`.quiet();

    const res = await run(["report", "annual", "--company", company, "--from", "2025-01-01", "--to", "2025-12-31", "--format", "json"]);
    expect(res.exitCode).toBe(1);
    const parsed = JSON.parse(res.stdout);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.length).toBeGreaterThan(0);

    rmSync(root, { recursive: true, force: true });
  });
});
