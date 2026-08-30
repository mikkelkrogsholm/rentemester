import { describe, expect, test } from "bun:test";
import { cpSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { resolveSource } from "../../src/core/import/source";
import { dineroParser } from "../../src/core/import/dinero";
import { runImportFromSource } from "../../src/core/import/framework";
import { getMigrationOpenItems } from "../../src/core/migration-open-items";
import { importedReceivableBalanceOre } from "../../src/core/imported-receivables";

const FIXTURE = join(import.meta.dir, "../../examples/import-dinero");

function exportWithReceivable(amount = "24500,000000") {
  const root = mkdtempSync(join(tmpdir(), "rentemester-dinero-open-items-source-"));
  cpSync(FIXTURE, root, { recursive: true });
  writeFileSync(
    join(root, "2025", "SaldoBalance.csv"),
    `Konto;Kontonavn;Beløb\n5520;Tilgodehavender fra salg;${amount}\n`,
  );
  return root;
}

function company() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-dinero-open-items-company-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("Dinero aggregate open-item controls", () => {
  test("lands the explicit invoice/payment/credit-note companion schedule and reconciles it to SaldoBalance", () => {
    const sourceRoot=exportWithReceivable(); const c=company();
    try {
      const h=(letter:string)=>letter.repeat(64);
      writeFileSync(join(sourceRoot,"Rentemester-modtagerposter-v1.json"),JSON.stringify({contract:"rentemester-imported-receivables-v1",sourceDocumentHash:h("a"),invoices:[
        {id:"INV-opening",customerId:"C-1",invoiceDate:"2024-12-20",grossAmount:30000,controlAccountNo:"5520",recognitionRef:"opening-debtors",documentHash:h("b"),payments:[{id:"PAY-1",eventKind:"payment",paymentDate:"2025-03-05",amount:30000,paymentRef:"dinero-payment-1",documentHash:h("c")}]},
        {id:"INV-current",customerId:"C-2",invoiceDate:"2025-03-20",grossAmount:25000,controlAccountNo:"5520",recognitionRef:"dinero-invoice-2",documentHash:h("d"),payments:[{id:"CN-1",eventKind:"credit_note",paymentDate:"2025-03-21",amount:500,paymentRef:"dinero-credit-1",documentHash:h("e")}]}]}));
      const landed=runImportFromSource(c.db,dineroParser,sourceRoot,{createdBy:"agent:test",companyRoot:c.root});
      expect(landed.ok,landed.errors.join("; ")).toBe(true);
      expect(importedReceivableBalanceOre(c.db,"2025-03-31","5520").total).toBe(2_450_000n);
      expect(c.db.query("SELECT event_kind,amount FROM imported_receivable_events ORDER BY id").all()).toEqual([{event_kind:"payment",amount:30000},{event_kind:"credit_note",amount:500}]);
      expect(verifyAuditChain(c.db).ok).toBe(true);
    } finally { c.db.close(); rmSync(c.root,{recursive:true,force:true}); rmSync(sourceRoot,{recursive:true,force:true}); }
  });

  test("parses the latest-year SaldoBalance without claiming item-level evidence", () => {
    const sourceRoot = exportWithReceivable();
    try {
      const parsed = dineroParser.parseSource!(resolveSource(sourceRoot));
      expect(parsed.errors).toEqual([]);
      expect(parsed.source?.openItemControlBalances).toEqual([{
        accountNo: "5520",
        kind: "receivable",
        amount: 24_500,
        sourceReference: "2025/SaldoBalance.csv",
      }]);
    } finally {
      rmSync(sourceRoot, { recursive: true, force: true });
    }
  });

  test("previews without mutation, then atomically preserves one unallocated control batch", () => {
    const sourceRoot = exportWithReceivable();
    const c = company();
    try {
      const dry = runImportFromSource(c.db, dineroParser, sourceRoot, {
        createdBy: "agent:test",
        companyRoot: c.root,
        dryRun: true,
      });
      expect(dry).toMatchObject({
        ok: true,
        dryRun: true,
        migrationOpenItems: { batchCount: 1, receivableAmount: 24_500, payableAmount: 0 },
      });
      expect(c.db.query("SELECT COUNT(*) AS count FROM migration_open_item_batches").get()).toEqual({ count: 0 });

      const landed = runImportFromSource(c.db, dineroParser, sourceRoot, {
        createdBy: "agent:test",
        companyRoot: c.root,
      });
      expect(landed).toMatchObject({
        ok: true,
        migrationOpenItems: { batchCount: 1, receivableAmount: 24_500, payableAmount: 0 },
      });
      expect(getMigrationOpenItems(c.db).rows).toEqual([
        expect.objectContaining({
          externalRef: "UNALLOCATED:5520",
          counterpartyName: null,
          issueDate: null,
          openBalance: 24_500,
          sourceKind: "control_balance",
          resolutionStatus: "unallocated",
        }),
      ]);
      expect(verifyAuditChain(c.db).ok).toBe(true);
    } finally {
      c.db.close();
      rmSync(c.root, { recursive: true, force: true });
      rmSync(sourceRoot, { recursive: true, force: true });
    }
  });

  test("fails closed and rolls ledger data back when SaldoBalance differs by one øre", () => {
    const sourceRoot = exportWithReceivable("24500,010000");
    const c = company();
    try {
      const landed = runImportFromSource(c.db, dineroParser, sourceRoot, {
        createdBy: "agent:test",
        companyRoot: c.root,
      });
      expect(landed.ok).toBe(false);
      expect(landed.errors.join(" ")).toContain("does not reconcile to the imported ledger in øre");
      expect(c.db.query("SELECT COUNT(*) AS count FROM journal_entries").get()).toEqual({ count: 0 });
      expect(c.db.query("SELECT COUNT(*) AS count FROM migration_open_item_batches").get()).toEqual({ count: 0 });
      expect(c.db.query("SELECT outcome FROM dinero_import_attempts").get()).toEqual({ outcome: "rejected" });
    } finally {
      c.db.close();
      rmSync(c.root, { recursive: true, force: true });
      rmSync(sourceRoot, { recursive: true, force: true });
    }
  });
});
