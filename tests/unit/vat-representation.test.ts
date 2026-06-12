// Tests: src/core/vat.ts (VAT representation)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { buildVatReport, postRepresentationPurchase } from "../../src/core/vat";

import { cleanupDir } from "../helpers/cleanup";
describe("representation VAT", () => {
  test("posts a representation purchase with only 25 percent deductible input VAT", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-repr-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-repr-inbox-"));
    const sourceFile = join(inbox, "restaurant.txt");
    writeFileSync(sourceFile, "Restaurant receipt\n1250 DKK\n");

    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    seedAccounts(db);

    const doc = ingestDocument(db, root, sourceFile, {
      source: "email",
      issueDate: "2026-05-18",
      invoiceNo: "REST-1",
      deliveryDescription: "Restaurant representation",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Restaurant ApS", address: "København", vatOrCvr: "DK99887766" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Firmakort"
    });
    expect(doc.ok).toBe(true);

    const posted = postRepresentationPurchase(db, {
      transactionDate: "2026-05-18",
      text: "Client dinner",
      documentId: doc.documentId!,
      netAmount: 1000,
    });

    expect(posted.ok).toBe(true);
    expect(posted.appliedRules).toContain("DK-VAT-REPRESENTATION-001");

    const lines = db.query(
      `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
       WHERE jl.journal_entry_id = ? ORDER BY jl.id ASC`
    ).all(posted.entryId!) as any[];
    expect(lines).toHaveLength(4);
    expect(lines[0]).toMatchObject({ account_no: "3070", debit_amount: 1000, vat_code: "REPRESENTATION_SPECIAL" });
    expect(lines[1]).toMatchObject({ account_no: "3070", debit_amount: 187.5, credit_amount: 0 });
    expect(lines[2]).toMatchObject({ account_no: "4000", debit_amount: 62.5, credit_amount: 0 });
    expect(lines[3]).toMatchObject({ account_no: "2000", debit_amount: 0, credit_amount: 1250 });

    const vat = buildVatReport(db, "2026-05-01", "2026-05-31");
    expect(vat.ok).toBe(true);
    expect(vat.inputVat).toBe(62.5);
    expect(vat.representationPurchaseBase).toBe(1000);
    expect(vat.netVatPayable).toBe(-62.5);

    const chain = verifyAuditChain(db);
    expect(chain.ok).toBe(true);

    db.close();
    cleanupDir(root);
    cleanupDir(inbox);
  });
});
