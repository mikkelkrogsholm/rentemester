// Tests: src/core/tax-return.ts
// Corporate taxable-income preparation (oplysningsskema) — a deterministic
// FIRST SLICE that derives skattepligtig indkomst from the bookkept annual
// result plus the skattemæssige reguleringer the ledger can see deterministically.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { closeAccountingPeriod } from "../../src/core/periods";
import { postRepresentationPurchase } from "../../src/core/vat";
import { registerAsset, postDepreciationPeriod } from "../../src/core/assets";
import { buildTaxReturn } from "../../src/core/tax-return";

function newCompany(prefix: string, cvr: string | null = "DK12345678", companyForm = "Anpartsselskab") {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const inbox = mkdtempSync(join(tmpdir(), `${prefix}inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.query(
    `INSERT INTO companies (id, name, country, currency, cvr, company_form, fiscal_year_start_month, fiscal_year_label_strategy)
     VALUES (1, 'Rentemester ApS', 'DK', 'DKK', ?, ?, 1, 'end-year')`,
  ).run(cvr, companyForm);
  return { root, inbox, db };
}

function ingestDoc(db: ReturnType<typeof openDb>, root: string, inbox: string, name: string): number {
  const sourceFile = join(inbox, `${name}.txt`);
  // Distinct per-document content so the SHA-256 dedup check never collides.
  writeFileSync(sourceFile, `Bilag ${name}\n1250 DKK\n`);
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2025-06-15",
    invoiceNo: `TAX-${name}`,
    deliveryDescription: "Ydelse",
    amountIncVat: 1250,
    currency: "DKK",
    sender: { name: "Leverandor", address: "Saelgervej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 250,
    paymentDetails: "Bankoverforsel",
  });
  expect(doc.ok).toBe(true);
  return doc.documentId!;
}

// Posts a simple, balanced profitable year: an equity contribution, revenue and
// a software expense. Revenue 1000, expense 400 -> bookkept result 600.
function postYear(db: ReturnType<typeof openDb>, root: string, inbox: string) {
  const docId = ingestDoc(db, root, inbox, "year");
  const open = postJournalEntry(db, {
    transactionDate: "2025-01-02",
    text: "Indskud egenkapital",
    lines: [
      { accountNo: "2000", debitAmount: 50000 },
      { accountNo: "5000", creditAmount: 50000 },
    ],
  });
  expect(open.ok).toBe(true);
  const sale = postJournalEntry(db, {
    transactionDate: "2025-06-15",
    text: "Konsulentsalg",
    documentId: docId,
    lines: [
      { accountNo: "2000", debitAmount: 1250 },
      { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
      { accountNo: "1200", creditAmount: 250 },
    ],
  });
  expect(sale.ok).toBe(true);
  const expense = postJournalEntry(db, {
    transactionDate: "2025-09-10",
    text: "Softwarekob",
    documentId: docId,
    lines: [
      { accountNo: "3000", debitAmount: 400, vatCode: "DK_PURCHASE_25" },
      { accountNo: "4000", debitAmount: 100 },
      { accountNo: "2000", creditAmount: 500 },
    ],
  });
  expect(expense.ok).toBe(true);
}

function lockYear(db: ReturnType<typeof openDb>) {
  const closed = closeAccountingPeriod(db, {
    periodStart: "2025-01-01",
    periodEnd: "2025-12-31",
    kind: "fiscal_year",
    status: "closed",
    createdBy: "agent:test",
  });
  expect(closed.ok).toBe(true);
}

function cleanup(...dirs: string[]) {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
}

describe("buildTaxReturn (oplysningsskema preparation, micro-ApS)", () => {
  test("derives skattepligtig indkomst and 22% selskabsskat for a locked year", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-ok-");
    postYear(db, root, inbox);
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
    expect(result.fiscalYearStart).toBe("2025-01-01");
    expect(result.fiscalYearEnd).toBe("2025-12-31");

    // Bookkept result: revenue 1000 - expense 400 = 600.
    expect(result.bookkeptResult).toBe(600);
    // No representation, no assets -> no deterministic adjustments.
    expect(result.adjustments).toEqual([]);
    expect(result.totalAdjustments).toBe(0);
    // Taxable income = bookkept result + adjustments.
    expect(result.taxableIncome).toBe(600);
    // Corporate tax: 22% of 600 = 132.
    expect(result.corporateTaxRate).toBe(0.22);
    expect(result.corporateTax).toBe(132);

    // Conservative claim language.
    expect(result.preparedBy).toBe("Rentemester");
    expect(result.disclaimer.toLowerCase()).toContain("oplysningsskema");

    db.close();
    cleanup(root, inbox);
  });

  test("adds back non-deductible representation as a deterministic adjustment", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-repr-");
    postYear(db, root, inbox);
    // Representation purchase: net 1000, 25% VAT = 250, only 25% deductible.
    // Non-deductible VAT = 250 * 0.75 = 187.50 is expensed (account 3070) and
    // is not tax-deductible -> it must be added back.
    const reprDoc = ingestDoc(db, root, inbox, "repr");
    const repr = postRepresentationPurchase(db, {
      transactionDate: "2025-08-01",
      text: "Kundemiddag",
      documentId: reprDoc,
      netAmount: 1000,
    });
    expect(repr.ok).toBe(true);
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(true);
    // One deterministic adjustment: the non-deductible representation VAT.
    const addBack = result.adjustments.find((a) => a.kind === "non_deductible_representation");
    expect(addBack).toBeDefined();
    expect(addBack!.amount).toBe(187.5);
    expect(result.totalAdjustments).toBe(187.5);
    // Bookkept result drops by the expensed representation (base 1000 + non-deductible
    // VAT 187.50 hit the P&L); taxable income adds the non-deductible VAT back.
    expect(result.taxableIncome).toBe(result.bookkeptResult + 187.5);

    db.close();
    cleanup(root, inbox);
  });

  test("flags book depreciation as needs-review (tax depreciation is not deterministic)", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-depr-");
    postYear(db, root, inbox);
    const assetDoc = ingestDoc(db, root, inbox, "asset");
    const asset = registerAsset(db, {
      name: "Laptop",
      category: "hardware",
      acquisitionDate: "2025-02-01",
      cost: 24000,
      usefulLifeMonths: 24,
      purchaseDocumentId: assetDoc,
    });
    expect(asset.ok).toBe(true);
    const dep = postDepreciationPeriod(db, {
      assetId: asset.assetId!,
      periodIndex: 1,
      transactionDate: "2025-03-01",
    });
    expect(dep.ok).toBe(true);
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(true);
    // Book depreciation is posted, so the depreciation difference must be a
    // needs-review item — Rentemester does not guess saldoafskrivning.
    const review = result.needsReview.find((r) => r.kind === "depreciation_difference");
    expect(review).toBeDefined();
    expect(review!.bookDepreciation).toBeGreaterThan(0);
    // The needs-review item is NOT silently folded into taxable income.
    expect(result.taxableIncome).toBe(result.bookkeptResult + result.totalAdjustments);

    db.close();
    cleanup(root, inbox);
  });

  test("flags a non-ApS company form as needs-review and computes no corporate tax", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-form-", "DK12345678", "Enkeltmandsvirksomhed");
    postYear(db, root, inbox);
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    // The slice only handles the micro-ApS case; anything else is needs-review.
    expect(result.ok).toBe(true);
    expect(result.corporateTax).toBeNull();
    expect(result.needsReview.some((r) => r.kind === "company_form_out_of_scope")).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("computes no positive tax on a loss, flags the loss carry-forward as needs-review", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-loss-");
    const docId = ingestDoc(db, root, inbox, "loss");
    postJournalEntry(db, {
      transactionDate: "2025-01-02",
      text: "Indskud",
      lines: [
        { accountNo: "2000", debitAmount: 50000 },
        { accountNo: "5000", creditAmount: 50000 },
      ],
    });
    // Revenue 1000, expense 4000 -> loss of 3000.
    postJournalEntry(db, {
      transactionDate: "2025-06-15",
      text: "Salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    postJournalEntry(db, {
      transactionDate: "2025-09-10",
      text: "Stort softwarekob",
      documentId: docId,
      lines: [
        { accountNo: "3000", debitAmount: 4000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 1000 },
        { accountNo: "2000", creditAmount: 5000 },
      ],
    });
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(true);
    expect(result.taxableIncome).toBe(-3000);
    // A negative taxable income yields zero corporate tax this year.
    expect(result.corporateTax).toBe(0);
    // The carry-forward of the loss (fremført underskud) is a needs-review item.
    expect(result.needsReview.some((r) => r.kind === "tax_loss_carry_forward")).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("is deterministic: identical input yields a byte-identical report", () => {
    const a = newCompany("rentemester-tax-det-a-");
    const b = newCompany("rentemester-tax-det-b-");
    for (const c of [a, b]) {
      postYear(c.db, c.root, c.inbox);
      lockYear(c.db);
    }
    const resultA = buildTaxReturn(a.db, "2025-01-01", "2025-12-31");
    const resultB = buildTaxReturn(b.db, "2025-01-01", "2025-12-31");
    expect(JSON.stringify(resultA)).toBe(JSON.stringify(resultB));

    a.db.close();
    b.db.close();
    cleanup(a.root, a.inbox, b.root, b.inbox);
  });

  test("fails clearly when the fiscal year is not locked", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-unlocked-");
    postYear(db, root, inbox);
    // No closeAccountingPeriod call.

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => /låst|lukket|close/i.test(e))).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("fails clearly when company CVR master data is missing", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-nocvr-", null);
    postYear(db, root, inbox);
    lockYear(db);

    const result = buildTaxReturn(db, "2025-01-01", "2025-12-31");
    expect(result.ok).toBe(false);
    expect(result.errors.some((e) => /cvr/i.test(e))).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("rejects invalid fiscal-year dates", () => {
    const { root, inbox, db } = newCompany("rentemester-tax-baddate-");
    const result = buildTaxReturn(db, "not-a-date", "2025-12-31");
    expect(result.ok).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);

    db.close();
    cleanup(root, inbox);
  });
});
