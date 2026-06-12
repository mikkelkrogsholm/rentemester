// Tests: src/core/assets.ts (#124 fixed-asset depreciation workflow)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { sumDkk } from "../../src/core/money";
import { ingestDocument } from "../../src/core/documents";
import { cleanupDir } from "../helpers/cleanup";
import {
  registerAsset,
  computeDepreciationSchedule,
  postDepreciationPeriod,
  buildAssetRegisterReport,
} from "../../src/core/assets";

function setup(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const sourceFile = join(inbox, "asset.txt");
  writeFileSync(sourceFile, `Asset invoice ${label}\n`);
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2026-01-10",
    invoiceNo: `ASSET-${label}`,
    deliveryDescription: "Laptop",
    amountIncVat: 50000,
    currency: "DKK",
    sender: { name: "Hardware ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 10000,
    paymentDetails: "Bank transfer",
  });
  expect(doc.ok).toBe(true);
  const cleanup = () => {
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  };
  return { root, db, documentId: doc.documentId!, cleanup };
}

describe("asset depreciation schedule", () => {
  test("computes a deterministic linear schedule that sums to cost in integer ore", () => {
    const schedule = computeDepreciationSchedule({
      cost: 48000,
      acquisitionDate: "2026-01-01",
      usefulLifeMonths: 36,
      method: "linear",
    });
    expect(schedule.length).toBe(36);
    const total = sumDkk(schedule.map((p) => p.amount));
    expect(total).toBe(48000);
    // even split: 48000 / 36 = 1333.33..., last period carries the rounding remainder
    expect(schedule[0].amount).toBe(1333.33);
    expect(schedule[0].periodIndex).toBe(1);
    expect(schedule[35].periodIndex).toBe(36);
    // every amount positive
    expect(schedule.every((p) => p.amount > 0)).toBe(true);
  });

  test("schedule is identical on repeated calls (deterministic)", () => {
    const args = { cost: 10000, acquisitionDate: "2026-03-15", usefulLifeMonths: 12, method: "linear" as const };
    expect(computeDepreciationSchedule(args)).toEqual(computeDepreciationSchedule(args));
  });
});

describe("asset registration + depreciation posting", () => {
  test("registers a capitalized asset and posts a balanced depreciation entry", () => {
    const { db, documentId, cleanup } = setup("asset-depr-ok");
    const reg = registerAsset(db, {
      name: "MacBook Pro",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 40000,
      usefulLifeMonths: 40,
      purchaseDocumentId: documentId,
    });
    expect(reg.ok).toBe(true);
    expect(reg.assetId).toBeGreaterThan(0);
    expect(reg.totalPeriods).toBe(40);

    const posted = postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 1, transactionDate: "2026-02-01" });
    expect(posted.ok).toBe(true);
    expect(posted.entryId).toBeGreaterThan(0);
    expect(posted.periodAmount).toBe(1000);

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`,
    ).all(posted.entryId!) as any[];
    expect(lines).toEqual([
      { account_no: "5820", debit_amount: 1000, credit_amount: 0 },
      { account_no: "5810", debit_amount: 0, credit_amount: 1000 },
    ]);
    const debit = lines.reduce((s, l) => s + l.debit_amount, 0);
    const credit = lines.reduce((s, l) => s + l.credit_amount, 0);
    expect(debit).toBe(credit);
    cleanup();
  });

  test("blocks posting the same depreciation period twice", () => {
    const { db, documentId, cleanup } = setup("asset-depr-dup");
    const reg = registerAsset(db, {
      name: "Server",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 12000,
      usefulLifeMonths: 12,
      purchaseDocumentId: documentId,
    });
    expect(reg.ok).toBe(true);
    const first = postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 1, transactionDate: "2026-02-01" });
    expect(first.ok).toBe(true);
    const dup = postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 1, transactionDate: "2026-02-01" });
    expect(dup.ok).toBe(false);
    expect(dup.errors.join(" ")).toContain("already");
    expect(dup.entryId).toBeUndefined();
    cleanup();
  });

  test("rejects a period index outside the schedule", () => {
    const { db, documentId, cleanup } = setup("asset-depr-range");
    const reg = registerAsset(db, {
      name: "Printer",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 6000,
      usefulLifeMonths: 6,
      purchaseDocumentId: documentId,
    });
    expect(reg.ok).toBe(true);
    const tooHigh = postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 7, transactionDate: "2026-08-01" });
    expect(tooHigh.ok).toBe(false);
    expect(tooHigh.errors.join(" ")).toContain("period");
    cleanup();
  });

  test("asset register report shows accumulated depreciation and net book value", () => {
    const { db, documentId, cleanup } = setup("asset-depr-report");
    const reg = registerAsset(db, {
      name: "Laptop",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 24000,
      usefulLifeMonths: 24,
      purchaseDocumentId: documentId,
    });
    expect(reg.ok).toBe(true);
    postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 1, transactionDate: "2026-02-01" });
    postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 2, transactionDate: "2026-03-01" });

    const report = buildAssetRegisterReport(db);
    expect(report.ok).toBe(true);
    const row = report.assets.find((a) => a.assetId === reg.assetId);
    expect(row).toBeDefined();
    expect(row!.cost).toBe(24000);
    expect(row!.accumulatedDepreciation).toBe(2000);
    expect(row!.netBookValue).toBe(22000);
    expect(report.totals.cost).toBe(24000);
    expect(report.totals.accumulatedDepreciation).toBe(2000);
    cleanup();
  });

  test("rejects registering an asset that references a missing purchase document", () => {
    const { db, cleanup } = setup("asset-depr-nodoc");
    const reg = registerAsset(db, {
      name: "Ghost",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 12000,
      usefulLifeMonths: 12,
      purchaseDocumentId: 99999,
    });
    expect(reg.ok).toBe(false);
    expect(reg.errors.join(" ")).toContain("document");
    cleanup();
  });

  test("depreciation entry links to the asset's purchase document for the audit trail", () => {
    const { db, documentId, cleanup } = setup("asset-depr-audit");
    const reg = registerAsset(db, {
      name: "Camera",
      category: "hardware",
      acquisitionDate: "2026-01-10",
      cost: 12000,
      usefulLifeMonths: 12,
      purchaseDocumentId: documentId,
    });
    expect(reg.ok).toBe(true);
    const posted = postDepreciationPeriod(db, { assetId: reg.assetId!, periodIndex: 1, transactionDate: "2026-02-01" });
    expect(posted.ok).toBe(true);
    const entry = db.query("SELECT document_id FROM journal_entries WHERE id = ?").get(posted.entryId!) as { document_id: number };
    expect(entry.document_id).toBe(documentId);
    cleanup();
  });
});
