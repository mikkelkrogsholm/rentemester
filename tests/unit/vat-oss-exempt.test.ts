// Tests: OSS first slice (src/core/vat-oss.ts) and rubrik C VAT-exempt sales.
// Both extend buildVatReport (src/core/vat.ts) and buildVatFiling
// (src/core/vat-filing.ts) with new VAT codes.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { buildVatReport } from "../../src/core/vat";
import { buildVatFiling } from "../../src/core/vat-filing";
import { vatRubrikkerForPeriod } from "../../src/server/data/vat";
import { buildOssReport } from "../../src/core/vat-oss";
import { setCompanyVatPeriodType } from "../../src/core/periods";
import { closeAccountingPeriod } from "../helpers/close-period";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";

function newCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const inbox = mkdtempSync(join(tmpdir(), `${prefix}inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.run("INSERT INTO companies (id, name) VALUES (1, 'Test ApS')");
  expect(setCompanyVatPeriodType(db, "month").ok).toBe(true);
  return { root, inbox, db };
}

function ingest(db: ReturnType<typeof openDb>, root: string, inbox: string, invoiceNo: string) {
  const sourceFile = join(inbox, `${invoiceNo}.txt`);
  writeFileSync(sourceFile, "Invoice\n1000 DKK\n");
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2026-03-15",
    invoiceNo,
    deliveryDescription: "Ydelse",
    amountIncVat: 1000,
    currency: "DKK",
    sender: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    recipient: { name: "Kunde", address: "Kundevej 1", vatOrCvr: "DK99887766" },
    vatAmount: 0,
    paymentDetails: "Bankoverførsel",
  });
  expect(doc.ok).toBe(true);
  return doc.documentId!;
}

describe("OSS first slice (digital services to EU consumers)", () => {
  test("buildVatReport tracks OSS consumer-sales base separately and excludes it from output VAT", () => {
    const { root, inbox, db } = newCompany("rentemester-oss-");
    const docId = ingest(db, root, inbox, "INV-OSS-1");

    // A digital-service sale to an EU consumer: VAT belongs in the OSS scheme,
    // not the Danish momsangivelse. The Danish ledger books the sale to revenue
    // with the OSS_EU_CONSUMER code and no Danish output VAT on 1200.
    const oss = postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "E-bog solgt til tysk forbruger",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1000 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "OSS_EU_CONSUMER", text: "OSS digital service" },
      ],
    });
    expect(oss.ok).toBe(true);

    const report = buildVatReport(db, "2026-03-01", "2026-03-31");
    expect(report.ok).toBe(true);
    // OSS sales are tracked in their own base, NOT silently miscategorised.
    expect(report.ossConsumerSalesBase).toBe(1000);
    // OSS sales carry no Danish output VAT.
    expect(report.outputVat).toBe(0);
    // And do not land in the standard 25% sales base.
    expect(report.salesBase25).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("buildVatFiling keeps OSS sales out of the standard rubrikker", () => {
    const { root, inbox, db } = newCompany("rentemester-oss-filing-");
    const docId = ingest(db, root, inbox, "INV-OSS-2");

    // One ordinary domestic sale + one OSS consumer sale.
    postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Dansk salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "OSS salg til EU-forbruger",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 3000 },
        { accountNo: "1000", creditAmount: 3000, vatCode: "OSS_EU_CONSUMER" },
      ],
    });

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    // Only the domestic sale's VAT lands in salgsmoms — the OSS sale is excluded.
    expect(filing.rubrikker.salgsmoms).toBe(250);
    // OSS sales are NOT rubrik B (that is non-OSS cross-border) nor rubrik C.
    expect(filing.rubrikker.rubrikBVarerEuSalesList).toBe(0);
    expect(filing.rubrikker.rubrikC).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("buildOssReport produces a per-period OSS skeleton from real data", () => {
    const { root, inbox, db } = newCompany("rentemester-oss-report-");
    const docId = ingest(db, root, inbox, "INV-OSS-3");

    postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "OSS salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 2000 },
        { accountNo: "1000", creditAmount: 2000, vatCode: "OSS_EU_CONSUMER" },
      ],
    });

    const report = buildOssReport(db, "2026-01-01", "2026-03-31");
    expect(report.ok).toBe(true);
    expect(report.ossConsumerSalesBase).toBe(2000);
    expect(report.entryCount).toBe(1);
    // It is a deterministic skeleton — not a SKAT submission.
    expect(report.submission).toBe(false);
    expect(report.appliedRules).toContain("DK-VAT-OSS-001");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});

describe("rubrik C — VAT-exempt sales", () => {
  test("buildVatReport tracks exempt-sales base separately", () => {
    const { root, inbox, db } = newCompany("rentemester-exempt-");
    const docId = ingest(db, root, inbox, "INV-EX-1");

    // A VAT-exempt sale (momsloven §13) — booked to revenue with the
    // DK_SALE_EXEMPT code, no output VAT.
    const exempt = postJournalEntry(db, {
      transactionDate: "2026-03-08",
      text: "Momsfrit salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 5000 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_EXEMPT", text: "Momsfri ydelse" },
      ],
    });
    expect(exempt.ok).toBe(true);

    const report = buildVatReport(db, "2026-03-01", "2026-03-31");
    expect(report.ok).toBe(true);
    expect(report.exemptSalesBase).toBe(5000);
    // No output VAT on an exempt sale.
    expect(report.outputVat).toBe(0);
    expect(report.salesBase25).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("buildVatFiling computes rubrik C from exempt sales instead of hardcoded zero", () => {
    const { root, inbox, db } = newCompany("rentemester-exempt-filing-");
    const docId = ingest(db, root, inbox, "INV-EX-2");

    postJournalEntry(db, {
      transactionDate: "2026-03-08",
      text: "Momsfrit salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 5000 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_EXEMPT" },
      ],
    });

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    // Rubrik C is now derived from real exempt-sales data.
    expect(filing.rubrikker.rubrikC).toBe(5000);
    // No output VAT, no standard sales.
    expect(filing.rubrikker.salgsmoms).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("cockpit vatRubrikkerForPeriod reports rubrik C from exempt sales (CLI parity, not hardcoded 0)", () => {
    const { root, inbox, db } = newCompany("rentemester-exempt-cockpit-");
    const docId = ingest(db, root, inbox, "INV-EX-COCKPIT");

    postJournalEntry(db, {
      transactionDate: "2026-03-09",
      text: "Momsfrit salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 5000 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_EXEMPT" },
      ],
    });

    // The cockpit momsangivelse surface (works on an OPEN period) must show
    // the SAME rubrik C as the CLI's `vat momsangivelse` (buildVatFiling, which
    // needs the period closed). A stale hardcoded 0 here understated
    // §13-exempt sales for an owner filing from the cockpit.
    const cockpit = vatRubrikkerForPeriod(db, "2026-03-01", "2026-03-31");
    expect(cockpit.rubrikC).toBe(5000);

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);
    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(cockpit.rubrikC).toBe(filing.rubrikker.rubrikC);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("rubrik C stays 0 when there are no exempt sales", () => {
    const { root, inbox, db } = newCompany("rentemester-exempt-zero-");
    const docId = ingest(db, root, inbox, "INV-EX-3");

    postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Dansk salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(filing.rubrikker.rubrikC).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});

describe("partial deduction (§§37-38 delvis fradragsret) warning", () => {
  test("warns when a period has both exempt sales AND fully-deducted input VAT", () => {
    const { root, inbox, db } = newCompany("rentemester-partial-");
    const docId = ingest(db, root, inbox, "INV-PART-1");

    // Exempt sale (no output VAT).
    const exempt = postJournalEntry(db, {
      transactionDate: "2026-03-08",
      text: "Momsfrit salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 5000 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_EXEMPT" },
      ],
    });
    expect(exempt.ok).toBe(true);

    // A purchase with input VAT deducted at 100%.
    const purchase = postJournalEntry(db, {
      transactionDate: "2026-03-10",
      text: "Indkøb",
      documentId: docId,
      lines: [
        { accountNo: "3000", debitAmount: 800, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 200 },
        { accountNo: "2000", creditAmount: 1000 },
      ],
    });
    expect(purchase.ok).toBe(true);

    const report = buildVatReport(db, "2026-03-01", "2026-03-31");
    expect(report.ok).toBe(true);
    expect(report.exemptSalesBase).toBe(5000);
    expect(report.inputVat).toBe(200);
    // Warning-only: amounts are unchanged.
    expect(
      report.warnings.some((w) => w.includes("§§37-38") || w.toLowerCase().includes("delvis fradrag")),
    ).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("no partial-deduction warning when there are no exempt sales", () => {
    const { root, inbox, db } = newCompany("rentemester-partial-none-");
    const docId = ingest(db, root, inbox, "INV-PART-2");

    const purchase = postJournalEntry(db, {
      transactionDate: "2026-03-10",
      text: "Indkøb",
      documentId: docId,
      lines: [
        { accountNo: "3000", debitAmount: 800, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 200 },
        { accountNo: "2000", creditAmount: 1000 },
      ],
    });
    expect(purchase.ok).toBe(true);

    const report = buildVatReport(db, "2026-03-01", "2026-03-31");
    expect(report.ok).toBe(true);
    expect(
      report.warnings.some((w) => w.includes("§§37-38") || w.toLowerCase().includes("delvis fradrag")),
    ).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("the §§37-38 warning propagates into buildVatFiling", () => {
    const { root, inbox, db } = newCompany("rentemester-partial-filing-");
    const docId = ingest(db, root, inbox, "INV-PART-3");

    postJournalEntry(db, {
      transactionDate: "2026-03-08",
      text: "Momsfrit salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 5000 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_EXEMPT" },
      ],
    });
    postJournalEntry(db, {
      transactionDate: "2026-03-10",
      text: "Indkøb",
      documentId: docId,
      lines: [
        { accountNo: "3000", debitAmount: 800, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 200 },
        { accountNo: "2000", creditAmount: 1000 },
      ],
    });

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(
      filing.warnings.some((w) => w.includes("§§37-38") || w.toLowerCase().includes("delvis fradrag")),
    ).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});
