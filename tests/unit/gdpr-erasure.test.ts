// Tests: src/core/gdpr.ts (GDPR retention-respecting erasure — #184)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry, verifyAuditChain } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { createCustomer, createVendor } from "../../src/core/master-data";
import { buildGdprSubjectExport, eraseGdprSubject } from "../../src/core/gdpr";

import { cleanupDir } from "../helpers/cleanup";
function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${prefix}-`));
  const company = join(root, "company");
  const db = openDb(ensureCompanyDirs(company).db);
  migrate(db);
  seedAccounts(db);
  db.run(
    `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
     VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`,
  );
  return { root, company, db };
}

describe("GDPR erasure respects bookkeeping retention", () => {
  test("refuses to erase a customer whose data is still under retention", () => {
    const { root, company, db } = freshCompany("gdpr-erase-refuse");
    const docFile = join(root, "doc.txt");
    writeFileSync(docFile, "Vendor invoice under retention\n");

    createVendor(db, { name: "Aktiv Lev", vatOrCvr: "DK33445566" });
    const ingested = ingestDocument(db, company, docFile, {
      source: "email",
      issueDate: "2026-03-01",
      invoiceNo: "GDPR-ERASE-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Aktiv Lev", address: "Sælgervej 3", vatOrCvr: "DK33445566" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    postJournalEntry(db, {
      transactionDate: "2026-03-02",
      text: "GDPR erase expense",
      documentId: ingested.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 },
      ],
    });

    // asOf well inside the ~5-year retention window.
    const result = eraseGdprSubject(db, { cvr: "DK33445566", asOf: "2027-06-01" });
    db.close();
    cleanupDir(root);

    expect(result.ok).toBe(true);
    expect(result.erasedCount).toBe(0);
    expect(result.refusedCount).toBeGreaterThan(0);
    const refusedSources = new Set(result.refused.map((r) => r.source));
    expect(refusedSources.has("documents")).toBe(true);
    // Refusals must carry the retention deadline as a clear, legal reason.
    expect(result.refused.every((r) => typeof r.retainUntil === "string")).toBe(true);
    expect(result.refused.every((r) => /retention/i.test(r.reason))).toBe(true);
  });

  test("erases personal data once it is no longer under retention", () => {
    const { root, db } = freshCompany("gdpr-erase-allowed");

    // A customer with no linked documents / bank rows — nothing keeps it.
    const created = createCustomer(db, {
      name: "Forhenværende Kunde",
      address: "Gammelvej 5, 5000 Odense C",
      vatOrCvr: "DK44556677",
      email: "gammel@example.com",
    });
    expect(created.ok).toBe(true);

    // asOf far in the future so any conceivable retention has lapsed.
    const result = eraseGdprSubject(db, { cvr: "DK44556677", asOf: "2099-01-01" });
    expect(result.ok).toBe(true);
    expect(result.erasedCount).toBeGreaterThan(0);
    expect(result.refusedCount).toBe(0);

    // After erasure, the export no longer exposes the personal fields.
    const report = buildGdprSubjectExport(db, { cvr: "DK44556677", asOf: "2099-01-01" });
    db.close();
    cleanupDir(root);

    const customerRecord = report.records.find((r) => r.source === "customers");
    expect(customerRecord).toBeDefined();
    expect(customerRecord!.erased).toBe(true);
    expect(customerRecord!.personalData.name).not.toBe("Forhenværende Kunde");
    expect(customerRecord!.personalData.email).toBeNull();
    expect(customerRecord!.personalData.address).toBeNull();
  });

  test("a second erasure of an already-erased subject is idempotent", () => {
    const { root, db } = freshCompany("gdpr-erase-idem");
    createCustomer(db, { name: "Idem Kunde", vatOrCvr: "DK66778899", email: "idem@example.com" });

    const first = eraseGdprSubject(db, { cvr: "DK66778899", asOf: "2099-01-01" });
    expect(first.ok).toBe(true);
    expect(first.erasedCount).toBeGreaterThan(0);

    const second = eraseGdprSubject(db, { cvr: "DK66778899", asOf: "2099-01-01" });
    db.close();
    cleanupDir(root);

    expect(second.ok).toBe(true);
    expect(second.erasedCount).toBe(0);
    expect(second.alreadyErasedCount).toBeGreaterThan(0);
  });
});
