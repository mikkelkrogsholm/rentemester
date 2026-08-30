import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importedReceivableBalanceOre, recordImportedReceivableSchedule, validateImportedReceivableSchedule } from "../../src/core/imported-receivables";

const hash = (letter: string) => letter.repeat(64);
function db() { const value = new Database(":memory:"); value.exec("PRAGMA foreign_keys = ON"); migrate(value); seedAccounts(value); value.query("INSERT INTO dinero_import_sources(id,raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count) VALUES(1,?,1,?,0)").run(hash("a"),hash("b")); value.query("INSERT INTO dinero_import_inventories(id,source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes) VALUES(1,1,?,?,0,0,0)").run(hash("a"),hash("b")); value.query("INSERT INTO dinero_import_attempts(id,inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256) VALUES(1,1,1,?,'synthetic-v1','agent:test','2025-01-01','accepted',?)").run(hash("a"),hash("c")); return value; }
const schedule = { contract:"rentemester-imported-receivables-v1" as const, sourceDocumentHash:hash("d"), invoices:[
  { id:"INV-partial",customerId:"customer-1",customerName:"Synthetic customer",invoiceDate:"2025-01-01",dueDate:"2025-01-15",grossAmount:100,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("e"),payments:[{id:"PAY-1",paymentDate:"2025-01-10",amount:25,paymentRef:"voucher-1",documentHash:hash("f")}] },
  { id:"INV-paid",invoiceDate:"2025-01-02",grossAmount:20,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("1"),payments:[{id:"PAY-2",paymentDate:"2025-01-03",amount:20,paymentRef:"voucher-2",documentHash:hash("2")}] },
  { id:"INV-credit",invoiceDate:"2025-01-04",grossAmount:40,controlAccountNo:"1200",recognitionRef:"opening",documentHash:hash("3"),payments:[{id:"CN-1",eventKind:"credit_note" as const,paymentDate:"2025-01-05",amount:10,paymentRef:"credit-note-1",documentHash:hash("4")}] },
] };

describe("imported receivables v36", () => {
  test("keeps source-evidenced imported invoices and payments exact at arbitrary cutoffs", () => {
    const value=db(); expect(recordImportedReceivableSchedule(value,1,schedule)).toMatchObject({ok:true});
    expect(importedReceivableBalanceOre(value,"2025-01-02","1200").total).toBe(12000n);
    expect(importedReceivableBalanceOre(value,"2025-01-10","1200").total).toBe(10500n);
    expect(importedReceivableBalanceOre(value,"2025-01-31","1200").total).toBe(10500n);
    const evidence=importedReceivableBalanceOre(value,"2025-01-31","1200").evidence; expect(evidence).toHaveLength(3); expect(evidence[0]).toMatchObject({externalInvoiceId:"INV-partial",customerExternalId:"customer-1",sourceDocumentHash:hash("e")});
    value.close();
  });
  test("is replay-safe and fails closed on conflicts and amount-only invented data", () => {
    const value=db(); const first=recordImportedReceivableSchedule(value,1,schedule); expect(first.ok).toBe(true); expect(recordImportedReceivableSchedule(value,1,schedule)).toMatchObject({ok:true,scheduleHash:first.scheduleHash});
    expect(recordImportedReceivableSchedule(value,1,{...schedule,invoices:[{...schedule.invoices[0]!,grossAmount:101}]}).errors).toContain("imported receivable schedule conflicts with accepted source");
    expect(validateImportedReceivableSchedule({contract:"rentemester-imported-receivables-v1",sourceDocumentHash:hash("a"),invoices:[{id:"x",invoiceDate:"2025-01-01",grossAmount:1,controlAccountNo:"1200"}]}).ok).toBe(false); value.close();
  });
});
