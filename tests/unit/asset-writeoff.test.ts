// Tests: src/core/assets.ts (#125 immediate small-asset write-off / straksafskrivning)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { listExceptions } from "../../src/core/exceptions";
import { postImmediateWriteOff, STRAKSAFSKRIVNING_THRESHOLD_DKK } from "../../src/core/assets";

import { cleanupDir } from "../helpers/cleanup";
function setup(label: string, amountIncVat: number, opts: { withDoc?: boolean } = {}) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  let documentId: number | undefined;
  if (opts.withDoc !== false) {
    const sourceFile = join(inbox, "asset.txt");
    writeFileSync(sourceFile, `Small asset invoice ${label}\n`);
    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-01-10",
      invoiceNo: `WO-${label}`,
      deliveryDescription: "Small tool",
      amountIncVat,
      currency: "DKK",
      sender: { name: "Tool ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bank transfer",
    });
    expect(doc.ok).toBe(true);
    documentId = doc.documentId!;
  }
  const cleanup = () => {
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  };
  return { root, db, documentId, cleanup };
}

describe("immediate write-off (straksafskrivning)", () => {
  test("posts a balanced write-off entry for an eligible small purchase with explicit confirmation", () => {
    const { db, documentId, cleanup } = setup("wo-ok", 5000);
    const result = postImmediateWriteOff(db, {
      name: "Cordless drill",
      category: "tools",
      acquisitionDate: "2026-01-10",
      cost: 5000,
      purchaseDocumentId: documentId!,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: true,
      thresholdRuleSource: "SKAT afskrivningsloven smaaanskaffelser",
    });
    expect(result.ok).toBe(true);
    expect(result.entryId).toBeGreaterThan(0);
    expect(result.writeOffId).toBeGreaterThan(0);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(result.entryId!) as any[];
    const debit = lines.reduce((s, l) => s + l.debit_amount, 0);
    const credit = lines.reduce((s, l) => s + l.credit_amount, 0);
    expect(debit).toBe(credit);
    expect(debit).toBe(5000);

    // threshold/rule metadata is persisted on the record
    const row = db.query("SELECT confirmed, threshold_rule_source, threshold_dkk FROM asset_writeoffs WHERE id = ?")
      .get(result.writeOffId!) as { confirmed: number; threshold_rule_source: string; threshold_dkk: number };
    expect(row.confirmed).toBe(1);
    expect(row.threshold_rule_source).toContain("SKAT");
    expect(row.threshold_dkk).toBe(STRAKSAFSKRIVNING_THRESHOLD_DKK);
    cleanup();
  });

  test("blocks write-off without the explicit confirmation flag", () => {
    const { db, documentId, cleanup } = setup("wo-noconfirm", 5000);
    const result = postImmediateWriteOff(db, {
      name: "Drill",
      category: "tools",
      acquisitionDate: "2026-01-10",
      cost: 5000,
      purchaseDocumentId: documentId!,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: false,
      thresholdRuleSource: "SKAT afskrivningsloven smaaanskaffelser",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("confirm");
    expect(result.entryId).toBeUndefined();
    cleanup();
  });

  test("blocks write-off when the source-backed threshold/rule metadata is missing", () => {
    const { db, documentId, cleanup } = setup("wo-nosource", 5000);
    const result = postImmediateWriteOff(db, {
      name: "Drill",
      category: "tools",
      acquisitionDate: "2026-01-10",
      cost: 5000,
      purchaseDocumentId: documentId!,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: true,
      thresholdRuleSource: "   ",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("threshold");
    cleanup();
  });

  test("surfaces an exception when cost exceeds the small-asset threshold (uncertain eligibility)", () => {
    const overThreshold = STRAKSAFSKRIVNING_THRESHOLD_DKK + 10000;
    const { db, documentId, cleanup } = setup("wo-overthreshold", overThreshold);
    const result = postImmediateWriteOff(db, {
      name: "Expensive rig",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: overThreshold,
      purchaseDocumentId: documentId!,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: true,
      thresholdRuleSource: "SKAT afskrivningsloven smaaanskaffelser",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("threshold");
    // an exception is queued for advisor review
    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows.some((e) => e.type === "ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN")).toBe(true);
    cleanup();
  });

  test("surfaces an exception when documentation is missing", () => {
    const { db, cleanup } = setup("wo-nodoc", 5000, { withDoc: false });
    const result = postImmediateWriteOff(db, {
      name: "Undocumented tool",
      category: "tools",
      acquisitionDate: "2026-01-10",
      cost: 5000,
      purchaseDocumentId: 99999,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: true,
      thresholdRuleSource: "SKAT afskrivningsloven smaaanskaffelser",
    });
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("document");
    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows.some((e) => e.type === "ASSET_WRITEOFF_MISSING_DOCUMENTATION")).toBe(true);
    cleanup();
  });

  test("blocks a duplicate write-off for the same purchase document", () => {
    const { db, documentId, cleanup } = setup("wo-dup", 5000);
    const args = {
      name: "Drill",
      category: "tools",
      acquisitionDate: "2026-01-10",
      cost: 5000,
      purchaseDocumentId: documentId!,
      expenseAccountNo: "3120",
      transactionDate: "2026-01-12",
      confirmImmediateWriteOff: true,
      thresholdRuleSource: "SKAT afskrivningsloven smaaanskaffelser",
    };
    const first = postImmediateWriteOff(db, args);
    expect(first.ok).toBe(true);
    const dup = postImmediateWriteOff(db, args);
    expect(dup.ok).toBe(false);
    expect(dup.errors.join(" ")).toContain("already");
    cleanup();
  });
});
