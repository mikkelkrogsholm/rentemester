// Tests: src/core/gdpr.ts (GDPR erasure never breaks the audit chain — #184)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry, verifyAuditChain } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { createCustomer, createVendor } from "../../src/core/master-data";
import { eraseGdprSubject } from "../../src/core/gdpr";

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

describe("GDPR erasure keeps the ledger and audit chain verifiable", () => {
  test("audit chain still verifies after an allowed erasure with posted entries present", () => {
    const { root, company, db } = freshCompany("gdpr-chain");
    const docFile = join(root, "doc.txt");
    writeFileSync(docFile, "Vendor invoice with ledger entry\n");

    // A vendor with a posted journal entry — its document is under retention,
    // but a STANDALONE customer with no ledger ties can be erased.
    createVendor(db, { name: "Bogført Lev", vatOrCvr: "DK77889900" });
    const ingested = ingestDocument(db, company, docFile, {
      source: "email",
      issueDate: "2026-03-01",
      invoiceNo: "GDPR-CHAIN-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Bogført Lev", address: "Sælgervej 4", vatOrCvr: "DK77889900" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    const posted = postJournalEntry(db, {
      transactionDate: "2026-03-02",
      text: "GDPR chain expense",
      documentId: ingested.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 },
      ],
    });
    expect(posted.ok).toBe(true);

    // An unrelated, erasable customer.
    createCustomer(db, { name: "Slet Mig", vatOrCvr: "DK10101010", email: "slet@example.com" });

    const before = verifyAuditChain(db);
    expect(before.ok).toBe(true);

    const erase = eraseGdprSubject(db, { cvr: "DK10101010", asOf: "2099-01-01" });
    expect(erase.ok).toBe(true);
    expect(erase.erasedCount).toBeGreaterThan(0);

    // The whole point: the hash chain and bookkeeping integrity survive.
    const after = verifyAuditChain(db);
    expect(after.ok).toBe(true);
    expect(after.errors).toEqual([]);

    // journal_entries / journal_lines / audit_log are untouched by erasure.
    const entryCount = (db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n;
    expect(entryCount).toBeGreaterThan(0);

    db.close();
    cleanupDir(root);
  });

  test("a refused erasure leaves the audit chain intact and writes no tombstones", () => {
    const { root, company, db } = freshCompany("gdpr-chain-refuse");
    const docFile = join(root, "doc.txt");
    writeFileSync(docFile, "Retained vendor invoice\n");

    createVendor(db, { name: "Refuse Lev", vatOrCvr: "DK20202020" });
    const ingested = ingestDocument(db, company, docFile, {
      source: "email",
      issueDate: "2026-03-01",
      invoiceNo: "GDPR-CHAIN-REFUSE-1",
      deliveryDescription: "Bogføring",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Refuse Lev", address: "Sælgervej 5", vatOrCvr: "DK20202020" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
    });
    postJournalEntry(db, {
      transactionDate: "2026-03-02",
      text: "GDPR chain refuse expense",
      documentId: ingested.documentId,
      lines: [
        { accountNo: "3000", debitAmount: 1000, vatCode: "DK_PURCHASE_25" },
        { accountNo: "4000", debitAmount: 250 },
        { accountNo: "2000", creditAmount: 1250 },
      ],
    });

    const erase = eraseGdprSubject(db, { cvr: "DK20202020", asOf: "2027-06-01" });
    expect(erase.ok).toBe(true);
    expect(erase.erasedCount).toBe(0);
    expect(erase.refusedCount).toBeGreaterThan(0);

    const chain = verifyAuditChain(db);
    const tombstones = (db.query("SELECT COUNT(*) AS n FROM gdpr_erasures").get() as { n: number }).n;
    db.close();
    cleanupDir(root);

    expect(chain.ok).toBe(true);
    expect(tombstones).toBe(0);
  });
});
