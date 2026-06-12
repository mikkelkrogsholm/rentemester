// Tests: src/core/vat-filing.ts (momsangivelse / SKAT VAT return)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { buildVatFiling } from "../../src/core/vat-filing";
import { closeAccountingPeriod } from "../../src/core/periods";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
function newCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const inbox = mkdtempSync(join(tmpdir(), `${prefix}inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
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
    expect(filing.rubrikker.momstilsvar).toBe(50);
    expect(filing.rubrikker.rubrikA).toBe(0);
    expect(filing.rubrikker.rubrikB).toBe(0);
    expect(filing.rubrikker.rubrikC).toBe(0);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("maps EU service reverse charge into ydelseskob-udland and rubrik A", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-rc-");
    const docId = ingest(db, root, inbox, "INV-FIL-RC", "DE123456789");

    // EU service purchase via reverse charge: net 1000.
    // Contributes 250 to BOTH salgsmoms (reverse-charge output) and kobsmoms.
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
    // Reverse-charge output VAT lands in salgsmoms.
    expect(filing.rubrikker.salgsmoms).toBe(250);
    // No physical goods code exists, so foreign-goods VAT is 0.
    expect(filing.rubrikker.momsAfVarekobUdland).toBe(0);
    // EU service reverse charge VAT = 25% of 1000.
    expect(filing.rubrikker.momsAfYdelseskobUdland).toBe(250);
    // Reverse-charge input VAT is also deductible kobsmoms.
    expect(filing.rubrikker.kobsmoms).toBe(250);
    // momstilsvar = 250 (salg) + 250 (ydelseskob udland) - 250 (kobsmoms)
    expect(filing.rubrikker.momstilsvar).toBe(250);
    // Rubrik A = value of goods/services purchased abroad.
    expect(filing.rubrikker.rubrikA).toBe(1000);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
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
    expect(filing.rubrikker.momstilsvar).toBe(250);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
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
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("fails when the period does not exactly match a closed accounting period", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-mismatch-");
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
    cleanupDir(root);
    cleanupDir(inbox);
  });

  test("rejects invalid period dates", () => {
    const { root, inbox, db } = newCompany("rentemester-vatfiling-bad-");
    const filing = buildVatFiling(db, "not-a-date", "2026-03-31");
    expect(filing.ok).toBe(false);
    expect(filing.errors.length).toBeGreaterThan(0);
    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
