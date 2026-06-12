// Tests: src/cli/retention.ts, src/cli.ts (retention CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";

import { cleanupDir } from "../helpers/cleanup";
describe("retention status CLI", () => {
  test("reports expired material counts as of a chosen date", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-retention-cli-"));
    const company = join(root, "company");
    const docFile = join(root, "vendor.txt");
    writeFileSync(docFile, "Vendor invoice\n");

    const db = openDb(ensureCompanyDirs(company).db);
    migrate(db);
    seedAccounts(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`);
    const ingested = ingestDocument(db, company, docFile, {
      source: "email",
      issueDate: "2026-03-01",
      invoiceNo: "RET-CLI-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    expect(ingested.ok).toBe(true);
    const posted = postJournalEntry(db, {
      transactionDate: "2026-03-02",
      text: "CLI retention expense",
      documentId: ingested.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 },
      ],
    });
    expect(posted.ok).toBe(true);
    db.close();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "retention", "status", "--company", company, "--as-of", "2032-01-01"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.rows.find((row: any) => row.table === "documents").expired).toBe(1);
    expect(parsed.rows.find((row: any) => row.table === "journal_entries").expired).toBe(1);

    cleanupDir(root);
  });
});
