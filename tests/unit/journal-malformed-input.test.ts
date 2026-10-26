import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initialiseCompanyVolume } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { dryRunJournalEntry, postJournalEntry, validateJournalEntry, type JournalEntryInput } from "../../src/core/ledger";
import { companyPaths } from "../../src/core/paths";

const valid = { transactionDate: "2026-05-16", text: "Synthetic contribution", lines: [
  { accountNo: "2000", debitAmount: 125 }, { accountNo: "5000", creditAmount: 125 },
] };
const cases = [
  { ...valid, lines: {} },
  { ...valid, lines: [null, null] },
  { ...valid, currency: 123 },
  { ...valid, lines: [{ accountNo: "2000", debitAmount: 1e308 }, { accountNo: "5000", creditAmount: 1e308 }] },
];

test("malformed journal shapes and overflowing amounts return rejection without writes", () => {
  const root = mkdtempSync(join(tmpdir(), "rentemester-journal-malformed-"));
  initialiseCompanyVolume(root, { cvr: "DK12345678" });
  const db = openDb(companyPaths(root).db);
  const snapshot = () => ["journal_entries", "journal_lines", "audit_log", "sequences"].map(table => db.query(`SELECT * FROM ${table} ORDER BY rowid`).all());
  try {
    const before = snapshot();
    for (const malformed of cases) {
      for (const operation of [validateJournalEntry, dryRunJournalEntry, postJournalEntry]) {
        const result = operation(db, malformed as unknown as JournalEntryInput);
        expect(result.ok).toBe(false);
        expect(result.errors.length).toBeGreaterThan(0);
        expect(snapshot()).toEqual(before);
      }
    }
    expect(postJournalEntry(db, valid)).toMatchObject({ ok: true, entryNo: "2026-00001" });
  } finally { db.close(); rmSync(root, { recursive: true, force: true }); }
});

test("CLI journal dry-run returns structured rejection for malformed lines", async () => {
  const root = mkdtempSync(join(tmpdir(), "rentemester-journal-malformed-cli-"));
  try {
    initialiseCompanyVolume(root, { cvr: "DK12345678" });
    const input = join(root, "input.json");
    writeFileSync(input, JSON.stringify(cases[0]));
    const proc = Bun.spawn([process.execPath, "run", "src/cli.ts", "journal", "dry-run", "--company", root, "--input", input, "--format", "json"], { cwd: join(import.meta.dir, "../.."), stdout: "pipe", stderr: "pipe" });
    const [exit, stdout, stderr] = await Promise.all([proc.exited, new Response(proc.stdout).text(), new Response(proc.stderr).text()]);
    expect(exit).toBe(1);
    expect(JSON.parse(stdout)).toMatchObject({ ok: false, errors: ["at least two journal lines are required"] });
    expect(stderr).not.toContain("TypeError");
  } finally { rmSync(root, { recursive: true, force: true }); }
});
