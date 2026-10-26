// Tests: src/core/vat-filing.ts (momsangivelse / SKAT VAT return)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { buildVatFiling } from "../../src/core/vat-filing";
import { setCompanyVatPeriodType, type VatPeriodType } from "../../src/core/periods";
import { closeAccountingPeriod } from "../helpers/close-period";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";

function newCompany(prefix: string, cadence: VatPeriodType = "month") {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const inbox = mkdtempSync(join(tmpdir(), `${prefix}inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.run("INSERT INTO companies (id, name) VALUES (1, 'Test ApS')");
  expect(setCompanyVatPeriodType(db, cadence).ok).toBe(true);
  return { root, inbox, db };
}

function ingest(db: ReturnType<typeof openDb>, root: string, inbox: string, invoiceNo: string, vendorVat: string) {
  const sourceFile = join(inbox, `${invoiceNo}.txt`);
  writeFileSync(sourceFile, "Invoice\n1250 DKK\n");
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2026-03-15",
    invoiceNo,
    deliveryDescription: "Ydelse",
    amountIncVat: 1250,
    currency: "DKK",
    sender: { name: "Leverandør", address: "Sælgervej 1", vatOrCvr: vendorVat },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 250,
    paymentDetails: "Bankoverførsel",
  });
  expect(doc.ok).toBe(true);
  return doc.documentId!;
}

describe("vat momsangivelse (filing)", () => {
  test("maps known postings into the standard SKAT rubrikker for a closed period", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-");
    const docId = ingest(db, root, inbox, "INV-FIL-1", "DK11223344");

    // Domestic sale: salgsmoms 250 on base 1000.
    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Konsulentsalg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);

    // Domestic purchase: deductible input VAT 200 on base 800.
    const purchase = postJournalEntry(db, {
      transactionDate: "2026-03-10",
      text: "Softwarekøb",
      documentId: docId,
      lines: [
        { accountNo: "3000", debitAmount: 800, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 200 },
        { accountNo: "2000", creditAmount: 1000 },
      ],
    });
    expect(purchase.ok).toBe(true);

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
    expect(filing.periodStatus).toBe("closed");
    // Salgsmoms = output VAT on domestic sales.
    expect(filing.rubrikker.salgsmoms).toBe(250);
    // No abroad purchases here.
    expect(filing.rubrikker.momsAfVarekobUdland).toBe(0);
    expect(filing.rubrikker.momsAfYdelseskobUdland).toBe(0);
    expect(filing.rubrikker.kobsmoms).toBe(200);
    // momstilsvar = salgsmoms + udenlandsk moms - kobsmoms
    expect(filing.rubrikker.momsIAlt).toBe(50);
    expect(filing.rubrikker.rubrikAVarer).toBe(0);
    expect(filing.rubrikker.rubrikAYdelser).toBe(0);
    expect(filing.rubrikker.rubrikBVarerEuSalesList).toBe(0);
    expect(filing.rubrikker.rubrikC).toBe(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("maps EU service reverse charge into ydelseskob-udland and rubrik A", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-rc-");
    const docId = ingest(db, root, inbox, "INV-FIL-RC", "DE123456789");

    // EU service purchase via reverse charge: net 1000.
    // The reverse-charge output VAT belongs in "Moms af ydelseskøb i udlandet",
    // NOT in salgsmoms — salgsmoms on TastSelv is VAT on own sales only.
    const rc = postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "EU hosting",
      documentId: docId,
      lines: [
        { accountNo: "3020", debitAmount: 1000, vatCode: "EU_SERVICE_REVERSE_CHARGE", text: "EU service base" },
        { accountNo: "4000", debitAmount: 250, text: "Deductible reverse-charge input VAT" },
        { accountNo: "2000", creditAmount: 1000 },
        { accountNo: "1200", creditAmount: 250, text: "Reverse-charge output VAT" },
      ],
    });
    expect(rc.ok).toBe(true);

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-03-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "reported",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(filing.periodStatus).toBe("reported");
    // Salgsmoms covers VAT on own sales only — the reverse-charge output VAT
    // is reported in "Moms af ydelseskøb i udlandet", never double-counted
    // into salgsmoms (momsloven §46 jf. §37).
    expect(filing.rubrikker.salgsmoms).toBe(0);
    // No physical goods code exists, so foreign-goods VAT is 0.
    expect(filing.rubrikker.momsAfVarekobUdland).toBe(0);
    // EU service reverse charge VAT = 25% of 1000.
    expect(filing.rubrikker.momsAfYdelseskobUdland).toBe(250);
    // Reverse-charge input VAT is also deductible kobsmoms.
    expect(filing.rubrikker.kobsmoms).toBe(250);
    // momstilsvar = 0 (salg) + 250 (ydelseskob udland) - 250 (kobsmoms) = 0:
    // a pure reverse-charge purchase is VAT-neutral.
    expect(filing.rubrikker.momsIAlt).toBe(0);
    // The filing must agree with the raw VAT report's net payable.
    expect(filing.rubrikker.momsIAlt).toBe(filing.vatReport.netVatPayable);
    // Rubrik A = value of goods/services purchased abroad.
    expect(filing.rubrikker.rubrikAYdelser).toBe(1000);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("does not double-count reverse-charge output VAT into salgsmoms (mixed period)", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-mixed-");
    const docId = ingest(db, root, inbox, "INV-FIL-MIX", "DK11223344");

    // Domestic sale: salgsmoms 250 on base 1000.
    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Konsulentsalg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);

    // EU service purchase via reverse charge: net 1000 → output 250 + input 250.
    const rc = postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "EU hosting",
      documentId: docId,
      lines: [
        { accountNo: "3020", debitAmount: 1000, vatCode: "EU_SERVICE_REVERSE_CHARGE", text: "EU service base" },
        { accountNo: "4000", debitAmount: 250, text: "Deductible reverse-charge input VAT" },
        { accountNo: "2000", creditAmount: 1000 },
        { accountNo: "1200", creditAmount: 250, text: "Reverse-charge output VAT" },
      ],
    });
    expect(rc.ok).toBe(true);

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
    // Salgsmoms = VAT on own sales only (250) — the reverse-charge output VAT
    // sits exclusively in momsAfYdelseskobUdland.
    expect(filing.rubrikker.salgsmoms).toBe(250);
    expect(filing.rubrikker.momsAfYdelseskobUdland).toBe(250);
    expect(filing.rubrikker.kobsmoms).toBe(250);
    // momstilsvar = 250 + 250 - 250 = 250 — and it must equal the raw VAT
    // report's netVatPayable (the filing re-maps rubrikker, it never changes
    // the amount owed).
    expect(filing.rubrikker.momsIAlt).toBe(250);
    expect(filing.rubrikker.momsIAlt).toBe(filing.vatReport.netVatPayable);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("ydelseskob-udland uses the booked reverse-charge VAT, not 25% of the summed base (rounding)", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-rc-round-");
    const docId = ingest(db, root, inbox, "INV-FIL-RC-ROUND", "DE123456789");

    // Two EU service purchases of base 10.02 each. Per booking, the reverse-
    // charge VAT is rounded per transaction: percentOfDkk(10.02, 25) = 2.51,
    // so 5.02 in total is booked on account 1200. But 25% of the *summed* base
    // (20.04) is 5.01. The ydelseskob-udland rubrik must equal the 5.02 that
    // was actually booked, not the 5.01 the aggregate-base formula yields.
    for (const i of [1, 2]) {
      const rc = postJournalEntry(db, {
        transactionDate: `2026-03-1${i}`,
        text: `EU hosting ${i}`,
        documentId: docId,
        lines: [
          { accountNo: "3020", debitAmount: 10.02, vatCode: "EU_SERVICE_REVERSE_CHARGE", text: "EU service base" },
          { accountNo: "4000", debitAmount: 2.51, text: "Deductible reverse-charge input VAT" },
          { accountNo: "2000", creditAmount: 10.02 },
          { accountNo: "1200", creditAmount: 2.51, text: "Reverse-charge output VAT" },
        ],
      });
      expect(rc.ok).toBe(true);
    }

    // Domestic sale: salgsmoms exactly 250.00 on base 1000.
    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Konsulentsalg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);

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
    // Account 1200 holds 250 (own sale) + 2 × 2.51 (RC output) = 255.02.
    expect(filing.vatReport.outputVat).toBe(255.02);
    // Salgsmoms must be the true own-sale output VAT: 250.00 exactly.
    expect(filing.rubrikker.salgsmoms).toBe(250);
    // Ydelseskob-udland must equal the booked RC output VAT: 5.02 (not 5.01).
    expect(filing.rubrikker.momsAfYdelseskobUdland).toBe(5);
    // Invariant: momstilsvar must still equal the raw report's netVatPayable.
    expect(filing.rubrikker.momsIAlt).toBe(250);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("does not warn that EU goods acquisitions are unsupported", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-eugoods-");
    const docId = ingest(db, root, inbox, "INV-FIL-EUG", "DE123456789");

    const rc = postJournalEntry(db, {
      transactionDate: "2026-03-12",
      text: "EU purchase",
      documentId: docId,
      lines: [
        { accountNo: "3020", debitAmount: 1000, vatCode: "EU_SERVICE_REVERSE_CHARGE", text: "EU base" },
        { accountNo: "4000", debitAmount: 250, text: "Deductible reverse-charge input VAT" },
        { accountNo: "2000", creditAmount: 1000 },
        { accountNo: "1200", creditAmount: 250, text: "Reverse-charge output VAT" },
      ],
    });
    expect(rc.ok).toBe(true);

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
    // A service-only period has no goods VAT and no stale goods limitation.
    expect(filing.rubrikker.momsAfVarekobUdland).toBe(0);
    expect(filing.warnings).toEqual([]);
    // Invariant unchanged: momstilsvar still equals the raw report net payable.
    expect(filing.rubrikker.momsIAlt).toBe(filing.vatReport.netVatPayable);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("keeps a domestic-only filing free of EU-goods warnings", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-noeugoods-");
    const docId = ingest(db, root, inbox, "INV-FIL-NOEUG", "DK11223344");
    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Konsulentsalg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);
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
      filing.warnings.some(
        (w) => w.toLowerCase().includes("varekøb") || w.toLowerCase().includes("§11") || w.toLowerCase().includes("erhvervelsesmoms"),
      ),
    ).toBe(false);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("filters postings by VAT period", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-period-");
    const docId = ingest(db, root, inbox, "INV-FIL-P", "DK11223344");

    // In-period sale.
    const inPeriod = postJournalEntry(db, {
      transactionDate: "2026-03-20",
      text: "In-period sale",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(inPeriod.ok).toBe(true);

    // Out-of-period sale (April) must not be counted.
    const outOfPeriod = postJournalEntry(db, {
      transactionDate: "2026-04-05",
      text: "Out-of-period sale",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 6250 },
        { accountNo: "1000", creditAmount: 5000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 1250 },
      ],
    });
    expect(outOfPeriod.ok).toBe(true);

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
    expect(filing.rubrikker.salgsmoms).toBe(250);
    expect(filing.rubrikker.momsIAlt).toBe(250);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("fails clearly when the VAT period is still open (not closed/reported)", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-open-");
    const docId = ingest(db, root, inbox, "INV-FIL-OPEN", "DK11223344");

    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-05",
      text: "Sale in still-open period",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);

    // No closeAccountingPeriod call -> period is open.
    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(false);
    expect(filing.periodStatus).toBe("open");
    expect(filing.errors.length).toBeGreaterThan(0);
    expect(filing.errors.some((e) => e.toLowerCase().includes("open") || e.toLowerCase().includes("not closed"))).toBe(true);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("fails when the period does not exactly match a closed accounting period", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-mismatch-", "quarter");
    ingest(db, root, inbox, "INV-FIL-MM", "DK11223344");

    // Close the full quarter, then ask for filing of a single month.
    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-01-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-03-01", "2026-03-31");
    expect(filing.ok).toBe(false);
    expect(filing.errors.length).toBeGreaterThan(0);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("accepts an exact registered monthly period with the monthly deadline", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-nonquarter-");
    const docId = ingest(db, root, inbox, "INV-FIL-NQ", "DK11223344");

    const sale = postJournalEntry(db, {
      transactionDate: "2026-02-10",
      text: "Salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-02-01",
      periodEnd: "2026-02-28",
      kind: "vat_period",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);

    const filing = buildVatFiling(db, "2026-02-01", "2026-02-28");
    expect(filing.ok).toBe(true);
    expect(filing.vatPeriodType).toBe("month");
    expect(filing.filingDeadline).toBe("2026-03-25");

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("does not warn about cadence for a standard calendar-quarter period", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-quarter-ok-", "quarter");
    const docId = ingest(db, root, inbox, "INV-FIL-QOK", "DK11223344");
    const sale = postJournalEntry(db, {
      transactionDate: "2026-03-10",
      text: "Salg",
      documentId: docId,
      lines: [
        { accountNo: "2000", debitAmount: 1250 },
        { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 250 },
      ],
    });
    expect(sale.ok).toBe(true);
    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-01-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "closed",
      createdBy: "agent:test",
    });
    expect(closed.ok).toBe(true);
    const filing = buildVatFiling(db, "2026-01-01", "2026-03-31");
    expect(filing.ok).toBe(true);
    expect(
      filing.warnings.some((w) => w.toLowerCase().includes("kvartal") || w.toLowerCase().includes("afregningsperiode")),
    ).toBe(false);

    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });

  test("rejects invalid period dates", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-bad-");
    const filing = buildVatFiling(db, "not-a-date", "2026-03-31");
    expect(filing.ok).toBe(false);
    expect(filing.errors.length).toBeGreaterThan(0);
    db.close();
    rmSync(root, { recursive: true, force: true });
    rmSync(inbox, { recursive: true, force: true });
  });
});
