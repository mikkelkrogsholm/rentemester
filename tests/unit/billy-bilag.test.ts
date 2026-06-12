import { expect, test, describe } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { initialiseCompanyVolume } from "../../src/core/company";
import { ingestBillyBilag } from "../../src/core/import/billy-bilag";
import type { ImportResult } from "../../src/core/import/types";

import { cleanupDir } from "../helpers/cleanup";
function freshCompany(): { root: string; db: Database } {
  const root = mkdtempSync(join(tmpdir(), "rentemester-billy-bilag-"));
  initialiseCompanyVolume(root, {});
  const db = new Database(join(root, "data", "ledger.sqlite"));
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function makeExportDir(
  files: Record<string, string | Uint8Array>,
  bilagFiles: Record<string, Uint8Array> = {},
): string {
  const dir = mkdtempSync(join(tmpdir(), "rentemester-billy-export-"));
  for (const [name, content] of Object.entries(files)) {
    if (typeof content === "string") writeFileSync(join(dir, name), content, "utf8");
    else writeFileSync(join(dir, name), content);
  }
  if (Object.keys(bilagFiles).length > 0) {
    const bilagDir = join(dir, "bilag");
    mkdirSync(bilagDir, { recursive: true });
    for (const [name, content] of Object.entries(bilagFiles)) {
      writeFileSync(join(bilagDir, name), content);
    }
  }
  return dir;
}

const FAKE_IMPORT_RESULT: ImportResult = {
  ok: true,
  sourceSystem: "billy",
  cutOverDate: "2026-01-01",
  openingBalanceLineCount: 0,
  historicalEntriesSkipped: 0,
  historicalEntriesPosted: [
    { voucherRef: "txn-abc", entryId: 1, entryNo: "2026-00001", transactionDate: "2026-01-15" },
    { voucherRef: "txn-def", entryId: 2, entryNo: "2026-00002", transactionDate: "2026-02-01" },
  ],
  auditTrail: [],
  appliedRules: [],
  errors: [],
};

describe("Billy bilag ingest", () => {
  test("returns early when no bill-transaction-map.json exists", () => {
    const { root, db } = freshCompany();
    const exportDir = makeExportDir({});
    try {
      const result = ingestBillyBilag(db, root, exportDir, FAKE_IMPORT_RESULT);
      expect(result.ok).toBe(true);
      expect(result.linked.length).toBe(0);
      expect(result.auditTrail[0]).toContain("No bill-transaction-map.json");
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });

  test("returns early when bilag/ directory is empty", () => {
    const { root, db } = freshCompany();
    const exportDir = makeExportDir({
      "bill-transaction-map.json": JSON.stringify([
        { billId: "bill-1", transactionId: "txn-abc", voucherNo: "100" },
      ]),
    });
    try {
      const result = ingestBillyBilag(db, root, exportDir, FAKE_IMPORT_RESULT);
      expect(result.ok).toBe(true);
      expect(result.linked.length).toBe(0);
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });

  test("ingests a bilag file and links it to a journal entry", () => {
    const { root, db } = freshCompany();
    // We need a real journal entry for the link. Seed one via a direct insert.
    db.run(
      `INSERT INTO journal_entries (id, entry_no, transaction_date, text, entry_hash, created_by_program, rule_version, created_by)
       VALUES (1, '2026-00001', '2026-01-15', 'Test entry', 'abc123', 'test', '1', 'user:test')`,
    );

    // Minimal valid PDF so MIME detection passes
    const fakeReceiptBytes = new TextEncoder().encode("%PDF-1.4\n1 0 obj\n<< >>\nendobj\n");

    const exportDir = makeExportDir(
      {
        "bill-transaction-map.json": JSON.stringify([
          { billId: "bill-1", transactionId: "txn-abc", voucherNo: "100" },
        ]),
      },
      { "bill-1__att-1.pdf": fakeReceiptBytes },
    );

    try {
      const importResult: ImportResult = {
        ...FAKE_IMPORT_RESULT,
        historicalEntriesPosted: [
          { voucherRef: "txn-abc", entryId: 1, entryNo: "2026-00001", transactionDate: "2026-01-15" },
        ],
      };

      const result = ingestBillyBilag(db, root, exportDir, importResult);
      expect(result.ok).toBe(true);
      expect(result.linked.length).toBe(1);
      expect(result.linked[0]!.billId).toBe("bill-1");
      expect(result.linked[0]!.transactionId).toBe("txn-abc");
      expect(result.linked[0]!.journalEntryNo).toBe("2026-00001");
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });

  test("reports unmatched when billId has no transactionId mapping", () => {
    const { root, db } = freshCompany();
    // Seed a journal entry so ingest can work
    db.run(
      `INSERT INTO journal_entries (id, entry_no, transaction_date, text, entry_hash, created_by_program, rule_version, created_by)
       VALUES (1, '2026-00001', '2026-01-15', 'Test entry', 'abc123', 'test', '1', 'user:test')`,
    );
    const fakeReceipt = new TextEncoder().encode("%PDF-1.4\n1 0 obj\n<< >>\nendobj\nUNMATCH");

    const exportDir = makeExportDir(
      {
        "bill-transaction-map.json": JSON.stringify([]),
      },
      { "unknown-bill__att-1.pdf": fakeReceipt },
    );

    try {
      const result = ingestBillyBilag(db, root, exportDir, FAKE_IMPORT_RESULT);
      expect(result.ok).toBe(true);
      expect(result.linked.length).toBe(0);
      expect(result.unmatched.length).toBe(1);
      expect(result.unmatched[0]!.billId).toBe("unknown-bill");
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });

  test("is idempotent — re-ingest skips already ingested files", () => {
    const { root, db } = freshCompany();
    db.run(
      `INSERT INTO journal_entries (id, entry_no, transaction_date, text, entry_hash, created_by_program, rule_version, created_by)
       VALUES (1, '2026-00001', '2026-01-15', 'Test entry', 'abc123', 'test', '1', 'user:test')`,
    );

    const fakeReceipt = new TextEncoder().encode("%PDF-1.4\n1 0 obj\n<< >>\nendobj\nIDEMPOTENT");
    const exportDir = makeExportDir(
      {
        "bill-transaction-map.json": JSON.stringify([
          { billId: "bill-1", transactionId: "txn-abc", voucherNo: "100" },
        ]),
      },
      { "bill-1__att-1.pdf": fakeReceipt },
    );

    const importResult: ImportResult = {
      ...FAKE_IMPORT_RESULT,
      historicalEntriesPosted: [
        { voucherRef: "txn-abc", entryId: 1, entryNo: "2026-00001", transactionDate: "2026-01-15" },
      ],
    };

    try {
      const first = ingestBillyBilag(db, root, exportDir, importResult);
      expect(first.linked.length).toBe(1);
      expect(first.duplicates.length).toBe(0);

      const second = ingestBillyBilag(db, root, exportDir, importResult);
      expect(second.linked.length).toBe(1);
      expect(second.duplicates.length).toBe(1);
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });

  test("rejects invalid bill-transaction-map.json", () => {
    const { root, db } = freshCompany();
    const exportDir = makeExportDir({
      "bill-transaction-map.json": "not json",
    });

    try {
      const result = ingestBillyBilag(db, root, exportDir, FAKE_IMPORT_RESULT);
      expect(result.ok).toBe(false);
      expect(result.errors[0]).toContain("invalid JSON");
      db.close();
    } finally {
      try { cleanupDir(root); } catch {}
      try { cleanupDir(exportDir); } catch {}
    }
  });
});
