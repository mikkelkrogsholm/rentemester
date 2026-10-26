import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { importBankCsv } from "../../src/core/bank";
import { migrate, openDb } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { bookExpenseFromBank } from "../../src/core/expense-booking";
import { dryRunJournalEntry, postJournalEntry, reverseJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { ensureCompanyDirs } from "../../src/core/paths";
import { buildVatReport } from "../../src/core/vat";
import { buildProfitAndLoss } from "../../src/core/financial-statements";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";
import { postOpeningBalance } from "../../src/core/opening-balance";
import { computePeriodCloseReadiness } from "../../src/core/period-close-readiness";

describe("internal vouchers backed by imported bank evidence (#554)", () => {
  test("books a 417 DKK bank fee without VAT and preserves immutable evidence", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-internal-voucher-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-internal-voucher-inbox-"));
    try {
      const csv = join(root, "bank.csv");
      const evidenceFile = join(inbox, "prepared-bank-fee.txt");
      writeFileSync(csv, [
        "transaction_date,booking_date,text,amount,currency,reference",
        "2026-07-31,2026-07-31,BANKGEBYR,-417,DKK,REF-FEE-417",
      ].join("\n"));
      writeFileSync(evidenceFile, "Internt bilag: bankgebyr 417,00 DKK\nIngen moms.\n");

      const db = openDb(ensureCompanyDirs(root).db);
      migrate(db);
      seedAccounts(db);
      expect(importBankCsv(db, root, csv)).toMatchObject({ ok: true, imported: 1 });
      const bank = db.query(
        "SELECT id FROM bank_transactions WHERE reference = 'REF-FEE-417'",
      ).get() as { id: number };

      const ingested = ingestDocument(db, root, evidenceFile, {
        source: "internal-preparation",
        documentType: "internal_voucher",
        issueDate: "2026-07-31",
        deliveryDescription: "Bankgebyr",
        amountIncVat: 417,
        vatAmount: 0,
        currency: "DKK",
        sourceBankTransactionId: bank.id,
        accountingRationale: "Bankgebyr ifølge importeret kontoudtog; ingen moms.",
      }, {
        createdBy: "agent:test",
        createdByProgram: "bun:test",
      });
      expect(ingested.ok).toBe(true);
      expect(typeof ingested.documentId).toBe("number");
      expect(ingested.documentId!).toBeGreaterThan(0);
      expect(ingested.sha256).toMatch(/^[a-f0-9]{64}$/);

      const evidence = db.query(
        `SELECT document_id, bank_transaction_id, accounting_rationale, prepared_by,
                prepared_by_program
           FROM internal_voucher_evidence
          WHERE bank_transaction_id = ?`,
      ).get(bank.id) as Record<string, unknown>;
      expect(evidence).toEqual({
        document_id: ingested.documentId,
        bank_transaction_id: bank.id,
        accounting_rationale: "Bankgebyr ifølge importeret kontoudtog; ingen moms.",
        prepared_by: "agent:test",
        prepared_by_program: "bun:test",
      });
      expect(db.query(
        "SELECT actor FROM audit_log WHERE event_type = 'document_ingest'",
      ).get()).toEqual({ actor: "agent:test via bun:test" });

      const wrongVat = bookExpenseFromBank(db, {
        documentId: ingested.documentId!,
        bankTransactionId: bank.id,
        expenseAccountNo: "3300",
        vatTreatment: "standard",
      });
      expect(wrongVat.ok).toBe(false);
      expect(wrongVat.errors.join(" ")).toContain("requires explicit vatTreatment exempt");

      const booked = bookExpenseFromBank(db, {
        documentId: ingested.documentId!,
        bankTransactionId: bank.id,
        expenseAccountNo: "3300",
        vatTreatment: "exempt",
        createdBy: "agent:test",
        createdByProgram: "bun:test",
      });
      expect(booked).toMatchObject({
        ok: true,
        grossAmount: 417,
        netAmount: 417,
        vatAmount: 0,
        vatTreatment: "exempt",
      });
      const lines = db.query(
        `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code
           FROM journal_lines jl
           JOIN accounts a ON a.id = jl.account_id
          WHERE jl.journal_entry_id = ?
          ORDER BY jl.id`,
      ).all(booked.entryId!);
      expect(lines).toEqual([
        { account_no: "3300", debit_amount: 417, credit_amount: 0, vat_code: null },
        { account_no: "2000", debit_amount: 0, credit_amount: 417, vat_code: null },
      ]);
      expect(buildVatReport(db, "2026-07-01", "2026-09-30")).toMatchObject({
        ok: true,
        inputVat: 0,
        outputVat: 0,
      });

      expect(() => db.run(
        "UPDATE internal_voucher_evidence SET accounting_rationale = 'changed' WHERE document_id = ?",
        ingested.documentId!,
      )).toThrow("append-only");
      expect(() => db.run(
        "DELETE FROM internal_voucher_evidence WHERE document_id = ?",
        ingested.documentId!,
      )).toThrow("append-only");
      expect(() => db.run(
        "UPDATE documents SET delivery_description = 'changed' WHERE id = ?",
        ingested.documentId!,
      )).toThrow("append-only");
      expect(() => db.run(
        "DELETE FROM documents WHERE id = ?",
        ingested.documentId!,
      )).toThrow("append-only");
      expect(() => db.run(
        "UPDATE bank_transactions SET amount = -418 WHERE id = ?",
        bank.id,
      )).toThrow("append-only");
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });

  test("fails closed when bank evidence is missing, inconsistent, reused, or incoming", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-internal-voucher-reject-"));
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-internal-voucher-reject-inbox-"));
    try {
      const db = openDb(ensureCompanyDirs(root).db);
      migrate(db);
      seedAccounts(db);
      db.query(
        `INSERT INTO bank_transactions
           (id, transaction_date, booking_date, text, amount, currency,
            reference, import_batch_id, source_file_hash, transaction_hash)
         VALUES
           (1, '2026-07-31', '2026-07-31', 'FEE', -417, 'DKK', 'OUT', 'batch', 'source', 'tx-out'),
           (2, '2026-07-31', '2026-07-31', 'REFUND', 417, 'DKK', 'IN', 'batch', 'source', 'tx-in')`,
      ).run();

      const ingest = (name: string, overrides: Record<string, unknown> = {}) => {
        const file = join(inbox, `${name}.txt`);
        writeFileSync(file, `synthetic ${name}`);
        return ingestDocument(db, root, file, {
          source: "internal-preparation",
          documentType: "internal_voucher",
          issueDate: "2026-07-31",
          deliveryDescription: "Bankgebyr",
          amountIncVat: 417,
          vatAmount: 0,
          currency: "DKK",
          sourceBankTransactionId: 1,
          accountingRationale: "Synthetic evidence",
          ...overrides,
        });
      };

      expect(ingest("missing-bank", { sourceBankTransactionId: 999 }).errors?.join(" "))
        .toContain("does not exist");
      expect(ingest("wrong-amount", { amountIncVat: 418 }).errors?.join(" "))
        .toContain("does not match bank transaction amount");
      expect(ingest("wrong-date", { issueDate: "2026-07-30" }).errors?.join(" "))
        .toContain("does not match bank transaction date");
      expect(ingest("incoming", { sourceBankTransactionId: 2 }).errors?.join(" "))
        .toContain("not an outgoing payment");
      expect(ingest("missing-rationale", { accountingRationale: "" }).errors?.join(" "))
        .toContain("accountingRationale is required");
      expect(ingest("vat", { vatAmount: 1 }).errors?.join(" "))
        .toContain("vatAmount must be exactly 0");

      const accepted = ingest("accepted");
      expect(accepted.ok).toBe(true);
      expect(ingest("reused").errors?.join(" ")).toContain("already backs internal voucher");
      expect(db.query("SELECT COUNT(*) AS count FROM documents").get()).toEqual({ count: 1 });
      db.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
      rmSync(inbox, { recursive: true, force: true });
    }
  });
});

describe("legacy opening creditor reclassification vouchers (#600)",()=>{
  test("permits only an unexplained, exact and append-only primobalance creditor correction",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-")),inbox=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-inbox-"));
    try{const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);
      const opening=postOpeningBalance(db,{cutOverDate:"2026-01-01",lines:[{accountNo:"5000",debitAmount:9630},{accountNo:"7000",creditAmount:9630}],createdBy:"agent:test",createdByProgram:"bun:test"});expect(opening.ok).toBe(true);
      const line=db.query("SELECT l.id FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? AND a.account_no='7000'").get(opening.entryId!) as {id:number};const file=join(inbox,"legacy.txt");writeFileSync(file,"synthetic legacy evidence");
      const voucher=ingestDocument(db,root,file,{source:"internal",documentType:"internal_voucher",internalVoucherKind:"legacy_opening_creditor_reclassification",issueDate:"2026-08-15",deliveryDescription:"Legacy creditor correction",amountIncVat:9630,vatAmount:0,currency:"DKK",accountingRationale:"Synthetic unexplained primobalance correction without bank, VAT or P&L.",legacyOpeningJournalEntryId:opening.entryId!,legacyOpeningJournalLineId:line.id},{createdBy:"agent:test",createdByProgram:"bun:test"});expect(voucher.ok).toBe(true);
      const payload={transactionDate:"2026-08-15",text:"Legacy creditor correction",documentId:voucher.documentId!,currency:"DKK",createdBy:"agent:test",createdByProgram:"bun:test",lines:[{accountNo:"7000",debitAmount:9630},{accountNo:"5000",creditAmount:9630}]};const pnl=buildProfitAndLoss(db,"2026-01-01","2026-12-31"),vat=buildVatReport(db,"2026-07-01","2026-09-30");
      expect(dryRunJournalEntry(db,payload)).toMatchObject({ok:true,accountEffects:expect.arrayContaining([expect.objectContaining({accountNo:"7000",balanceBefore:-9630,balanceAfter:0}),expect.objectContaining({accountNo:"5000",balanceBefore:9630,balanceAfter:0})])});const posted=postJournalEntry(db,payload);expect(posted).toMatchObject({ok:true,idempotent:false});expect(postJournalEntry(db,payload)).toMatchObject({ok:true,idempotent:true,entryId:posted.entryId});expect(buildProfitAndLoss(db,"2026-01-01","2026-12-31")).toEqual(pnl);const afterVat=buildVatReport(db,"2026-07-01","2026-09-30");expect({outputVat:afterVat.outputVat,inputVat:afterVat.inputVat,netVatPayable:afterVat.netVatPayable,rubrikker:afterVat.rubrikker}).toEqual({outputVat:vat.outputVat,inputVat:vat.inputVat,netVatPayable:vat.netVatPayable,rubrikker:vat.rubrikker});
      expect(()=>db.run("UPDATE legacy_opening_creditor_reclassification_evidence SET opening_journal_line_id=1")).toThrow("append-only");const backup=createSystemBackup(db,root,{createdAt:"2026-08-16T12:00:00.000Z"});expect(backup.ok).toBe(true);db.close();const restoredRoot=join(root,"restored");expect(restoreSystemBackup({backupDir:backup.backupDir!,targetCompanyRoot:restoredRoot})).toMatchObject({ok:true});const restored=openDb(ensureCompanyDirs(restoredRoot).db);expect(restored.query("SELECT document_id FROM legacy_opening_creditor_reclassification_evidence").get()).toEqual({document_id:voucher.documentId});restored.close();
    }finally{rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });
  test("keeps ordinary creditors and canonical payable evidence blocked",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-block-")),inbox=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-block-inbox-"));
    try{const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);const opening=postOpeningBalance(db,{cutOverDate:"2026-01-01",lines:[{accountNo:"5000",debitAmount:100},{accountNo:"7000",creditAmount:100}]});const line=db.query("SELECT l.id FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=? AND a.account_no='7000'").get(opening.entryId!) as {id:number};const file=join(inbox,"legacy-block.txt");writeFileSync(file,"synthetic");const voucher=ingestDocument(db,root,file,{source:"internal",documentType:"internal_voucher",internalVoucherKind:"legacy_opening_creditor_reclassification",issueDate:"2026-08-15",deliveryDescription:"Legacy creditor correction",amountIncVat:100,vatAmount:0,currency:"DKK",accountingRationale:"Synthetic",legacyOpeningJournalEntryId:opening.entryId!,legacyOpeningJournalLineId:line.id});const payload={transactionDate:"2026-08-15",text:"legacy",documentId:voucher.documentId!,lines:[{accountNo:"7000",debitAmount:100},{accountNo:"5000",creditAmount:100}]};expect(dryRunJournalEntry(db,{...payload,lines:[{accountNo:"7000",debitAmount:101},{accountNo:"5000",creditAmount:101}]}).ok).toBe(false);db.run("INSERT INTO payables(document_id,bill_date,due_date,gross_amount,net_amount,vat_amount,currency,journal_entry_id) VALUES(?,'2026-01-01','2026-01-02',1,1,0,'DKK',?)",voucher.documentId!,opening.entryId!);expect(dryRunJournalEntry(db,payload).errors.join(" ")).toContain("canonical payable evidence");db.close();}finally{rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });
  test("releases capacity only after one exact append-only reversal (#602)",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-reversal-")),inbox=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-reversal-inbox-"));
    try {
      const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);
      const opening=postOpeningBalance(db,{cutOverDate:"2026-01-01",lines:[{accountNo:"5000",debitAmount:100},{accountNo:"7000",creditAmount:100}],createdBy:"agent:test",createdByProgram:"bun:test"});expect(opening.ok).toBe(true);
      const line=(db.query("SELECT id FROM journal_lines WHERE journal_entry_id=? AND credit_amount=100").get(opening.entryId!) as {id:number}).id;
      const voucher=(name:string,date:string)=>{const file=join(inbox,`${name}.txt`);writeFileSync(file,name);const document=ingestDocument(db,root,file,{source:"internal",documentType:"internal_voucher",internalVoucherKind:"legacy_opening_creditor_reclassification",issueDate:date,deliveryDescription:name,amountIncVat:100,vatAmount:0,currency:"DKK",accountingRationale:"Synthetic documented opening creditor reclassification.",legacyOpeningJournalEntryId:opening.entryId!,legacyOpeningJournalLineId:line},{createdBy:"agent:test",createdByProgram:"bun:test"});expect(document.ok).toBe(true);return {documentId:document.documentId!,payload:{transactionDate:date,text:name,documentId:document.documentId!,currency:"DKK",createdBy:"agent:test",createdByProgram:"bun:test",lines:[{accountNo:"7000",debitAmount:100},{accountNo:"5000",creditAmount:100}]}};};
      const original=voucher("later correction","2026-08-15");const posted=postJournalEntry(db,original.payload);expect(posted.ok).toBe(true);
      const beforeReverse=voucher("replacement before reversal","2026-08-16");expect(dryRunJournalEntry(db,beforeReverse.payload).errors.join(" ")).toContain("exceeds the documented remaining");
      const reversal=reverseJournalEntry(db,{entryId:posted.entryId!,transactionDate:"2026-08-16",reason:"Move documented correction to the correct period",createdBy:"agent:test",createdByProgram:"bun:test"});expect(reversal.ok).toBe(true);
      expect(reverseJournalEntry(db,{entryId:reversal.entryId!,transactionDate:"2026-08-17",reason:"Chained reversal must be rejected",createdBy:"agent:test",createdByProgram:"bun:test"}).errors.join(" ")).toContain("already a reversal entry");
      const replacement=voucher("replacement after reversal","2026-01-02");const duplicate=voucher("competing replacement","2026-01-03");
      // Two independently prepared plans can both be valid. The immediate post
      // transaction must nevertheless let exactly one consume the released capacity.
      expect(dryRunJournalEntry(db,replacement.payload).ok).toBe(true);expect(dryRunJournalEntry(db,duplicate.payload).ok).toBe(true);
      const replacementPost=postJournalEntry(db,replacement.payload);expect(replacementPost).toMatchObject({ok:true,idempotent:false});expect(postJournalEntry(db,replacement.payload)).toMatchObject({ok:true,idempotent:true,entryId:replacementPost.entryId});
      expect(postJournalEntry(db,duplicate.payload).errors.join(" ")).toContain("exceeds the documented remaining");
      expect(verifyAuditChain(db,{companyRoot:root})).toMatchObject({ok:true,errors:[]});
      expect(computePeriodCloseReadiness(db,{periodStart:"2026-01-01",periodEnd:"2026-08-31",companyRoot:root}).controlsRun).toContain("LEDGER_AUDIT_CHAIN");
      db.close();
      const restoredRoot=join(root,"restored");const source=openDb(ensureCompanyDirs(root).db);const backup=createSystemBackup(source,root,{createdAt:"2026-08-17T12:00:00.000Z"});source.close();expect(backup.ok).toBe(true);expect(restoreSystemBackup({backupDir:backup.backupDir!,targetCompanyRoot:restoredRoot})).toMatchObject({ok:true});const restored=openDb(ensureCompanyDirs(restoredRoot).db);expect(verifyAuditChain(restored,{companyRoot:restoredRoot})).toMatchObject({ok:true,errors:[]});restored.close();
    } finally {rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });
  test("fails closed for partial or ambiguous legacy correction reversals (#602)",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-corrupt-reversal-")),inbox=mkdtempSync(join(tmpdir(),"rentemester-legacy-creditor-corrupt-reversal-inbox-"));
    try {
      const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);
      const opening=postOpeningBalance(db,{cutOverDate:"2026-01-01",lines:[{accountNo:"5000",debitAmount:200},{accountNo:"7000",creditAmount:100},{accountNo:"7000",creditAmount:100}],createdBy:"agent:test",createdByProgram:"bun:test"});
      const lines=db.query("SELECT id FROM journal_lines WHERE journal_entry_id=? AND credit_amount=100 ORDER BY id").all(opening.entryId!) as Array<{id:number}>;
      const createVoucher=(name:string,date:string,lineId=lines[0]!.id)=>{const file=join(inbox,`${name}.txt`);writeFileSync(file,name);const document=ingestDocument(db,root,file,{source:"internal",documentType:"internal_voucher",internalVoucherKind:"legacy_opening_creditor_reclassification",issueDate:date,deliveryDescription:name,amountIncVat:100,vatAmount:0,currency:"DKK",accountingRationale:"Synthetic documented opening creditor reclassification.",legacyOpeningJournalEntryId:opening.entryId!,legacyOpeningJournalLineId:lineId},{createdBy:"agent:test",createdByProgram:"bun:test"});expect(document.ok).toBe(true);return {transactionDate:date,text:name,documentId:document.documentId!,currency:"DKK",createdBy:"agent:test",createdByProgram:"bun:test",lines:[{accountNo:"7000",debitAmount:100},{accountNo:"5000",creditAmount:100}]};};
      const original=createVoucher("original correction","2026-08-15");const posted=postJournalEntry(db,original);expect(posted.ok).toBe(true);
      // This row has exact metadata and inverse lines but no canonical hash or
      // journal_reverse audit event. A corrupt/restored DB must not free capacity.
      db.run(`INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,created_by,created_by_program,status,reversal_of_entry_id,entry_hash) SELECT 900001,'SYN-FORGED','2026-08-16','Synthetic forged reversal',source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,'agent:test','bun:test','reversed',id,'not-a-real-hash' FROM journal_entries WHERE id=?`,posted.entryId!);
      db.run(`INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code,currency,text) SELECT 900001,account_id,credit_amount,debit_amount,vat_code,currency,text FROM journal_lines WHERE journal_entry_id=? ORDER BY id`,posted.entryId!);
      expect(dryRunJournalEntry(db,createVoucher("forged replacement","2026-01-02")).errors.join(" ")).toContain("partial or mismatched reversal evidence");
      const partialOriginal=createVoucher("partial original","2026-08-15",lines[1]!.id);const partialPosted=postJournalEntry(db,partialOriginal);expect(partialPosted.ok).toBe(true);
      const insertMalformedReversal=()=>{const id=(db.query("SELECT MAX(id)+1 AS id FROM journal_entries").get() as {id:number}).id;db.run(`INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,created_by,created_by_program,status,reversal_of_entry_id,entry_hash) SELECT ?,?,'2026-08-16','Synthetic malformed reversal',source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,'agent:test','bun:test','reversed',id,? FROM journal_entries WHERE id=?`,id,`SYN-REV-${id}`,`synthetic-${id}`,partialPosted.entryId!);db.run(`INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code,currency,text) SELECT ?,account_id,debit_amount,credit_amount,vat_code,currency,'Synthetic malformed reversal' FROM journal_lines WHERE journal_entry_id=? ORDER BY id LIMIT 1`,id,partialPosted.entryId!);};
      insertMalformedReversal();
      expect(dryRunJournalEntry(db,createVoucher("partial replacement","2026-01-03",lines[1]!.id)).errors.join(" ")).toContain("partial or mismatched reversal evidence");
      // The trigger prevents this in a healthy ledger. A restored/corrupt legacy
      // database with two reversal rows must still keep capacity consumed.
      db.exec("DROP TRIGGER journal_entries_reversal_shape_insert");insertMalformedReversal();
      expect(dryRunJournalEntry(db,createVoucher("ambiguous replacement","2026-01-04",lines[1]!.id)).errors.join(" ")).toContain("ambiguous reversal evidence");
      db.close();
    } finally {rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });
});

describe("non-cash balance correction vouchers (#599)",()=>{
  test("hash-binds one exact 9,630 DKK balance-only journal without changing P&L or VAT",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-non-cash-voucher-"));
    const inbox=mkdtempSync(join(tmpdir(),"rentemester-non-cash-voucher-inbox-"));
    try{
      const file=join(inbox,"balance-correction.txt");
      writeFileSync(file,"# Internt balancekorrektionsbilag\nBeløb: 9.630,00 DKK\nMoms: 0\n");
      const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);
      const ingested=ingestDocument(db,root,file,{source:"internal-preparation",documentType:"internal_voucher",internalVoucherKind:"non_cash_balance_correction",issueDate:"2026-08-15",deliveryDescription:"Dokumenteret balancekorrektion",amountIncVat:9630,vatAmount:0,currency:"DKK",accountingRationale:"Retter en historisk klassifikation mellem to balancekonti; ingen bankbevægelse eller moms."},{createdBy:"agent:test",createdByProgram:"bun:test"});
      expect(ingested.ok).toBe(true);
      expect(ingested.documentId).toEqual(expect.any(Number));
      expect(ingested.sha256).toMatch(/^[a-f0-9]{64}$/);
      const documentId=ingested.documentId!;
      const payload={transactionDate:"2026-08-15",text:"Dokumenteret ikke-kontant balancekorrektion",documentId,currency:"DKK",createdBy:"agent:test",createdByProgram:"bun:test",lines:[{accountNo:"5800",debitAmount:9630},{accountNo:"5000",creditAmount:9630}]};
      const beforeCounts=db.query("SELECT (SELECT COUNT(*) FROM journal_entries) journals,(SELECT COUNT(*) FROM audit_log) audit").get();
      const pnlBefore=buildProfitAndLoss(db,"2026-01-01","2026-12-31");const vatBefore=buildVatReport(db,"2026-07-01","2026-09-30");
      expect(dryRunJournalEntry(db,payload)).toMatchObject({ok:true,accountEffects:[expect.objectContaining({accountNo:"5000",delta:-9630}),expect.objectContaining({accountNo:"5800",delta:9630})]});
      expect(db.query("SELECT document_id FROM non_cash_balance_correction_evidence WHERE document_id=?").get(documentId)).toEqual({document_id:documentId});
      expect(db.query("SELECT (SELECT COUNT(*) FROM journal_entries) journals,(SELECT COUNT(*) FROM audit_log) audit").get()).toEqual(beforeCounts);
      const posted=postJournalEntry(db,payload);expect(posted.ok).toBe(true);expect(posted.idempotent).toBe(false);expect(posted.entryId).toEqual(expect.any(Number));
      expect(postJournalEntry(db,payload)).toMatchObject({ok:true,idempotent:true,entryId:posted.entryId});
      expect(buildProfitAndLoss(db,"2026-01-01","2026-12-31")).toEqual(pnlBefore);
      // The report's operational row counters may grow, but every VAT field and every filing rubrik is unchanged.
      const vatAfter=buildVatReport(db,"2026-07-01","2026-09-30");
      expect({outputVat:vatAfter.outputVat,inputVat:vatAfter.inputVat,netVatPayable:vatAfter.netVatPayable,rubrikker:vatAfter.rubrikker}).toEqual({outputVat:vatBefore.outputVat,inputVat:vatBefore.inputVat,netVatPayable:vatBefore.netVatPayable,rubrikker:vatBefore.rubrikker});
      expect(db.query("SELECT document_id,journal_entry_id,document_sha256,journal_entry_hash FROM non_cash_balance_correction_postings").get()).toMatchObject({document_id:ingested.documentId,journal_entry_id:posted.entryId,document_sha256:ingested.sha256,journal_entry_hash:posted.entryHash});
      expect(verifyAuditChain(db,{companyRoot:root})).toMatchObject({ok:true,errors:[]});
      expect(()=>db.run("UPDATE non_cash_balance_correction_evidence SET amount=1")).toThrow("append-only");expect(()=>db.run("DELETE FROM non_cash_balance_correction_postings")).toThrow("append-only");
      const backup=createSystemBackup(db,root,{createdAt:"2026-08-16T12:00:00.000Z"});
      expect(backup.ok).toBe(true);
      db.close();
      const restoredRoot=join(root,"restored");
      const restored=restoreSystemBackup({backupDir:backup.backupDir!,targetCompanyRoot:restoredRoot});
      expect(restored.ok).toBe(true);
      const restoredDb=openDb(ensureCompanyDirs(restoredRoot).db);
      expect(restoredDb.query("SELECT document_id,journal_entry_id FROM non_cash_balance_correction_postings").get()).toEqual({document_id:documentId,journal_entry_id:posted.entryId});
      expect(verifyAuditChain(restoredDb,{companyRoot:restoredRoot})).toMatchObject({ok:true,errors:[]});
      restoredDb.close();
    }finally{rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });

  test("fails closed on bank, VAT, P&L, mismatched and incomplete evidence",()=>{
    const root=mkdtempSync(join(tmpdir(),"rentemester-non-cash-reject-"));const inbox=mkdtempSync(join(tmpdir(),"rentemester-non-cash-reject-inbox-"));
    try{const db=openDb(ensureCompanyDirs(root).db);migrate(db);seedAccounts(db);let seq=0;
      const ingest=(overrides:Record<string,unknown>={})=>{const file=join(inbox,`v-${seq++}.txt`);writeFileSync(file,`synthetic ${seq}`);return ingestDocument(db,root,file,{source:"internal",documentType:"internal_voucher",internalVoucherKind:"non_cash_balance_correction",issueDate:"2026-08-15",deliveryDescription:"Balance correction",amountIncVat:100,vatAmount:0,currency:"DKK",accountingRationale:"Synthetic documented correction",...overrides});};
      expect(ingest({sourceBankTransactionId:1}).errors?.join(" ")).toContain("must not reference a bank");expect(ingest({accountingRationale:""}).ok).toBe(false);expect(ingest({vatAmount:1}).ok).toBe(false);expect(ingest({currency:"EUR"}).errors?.join(" ")).toContain("currency must be DKK");
      const doc=ingest();expect(doc.ok).toBe(true);const base={transactionDate:"2026-08-15",text:"correction",documentId:doc.documentId!,lines:[{accountNo:"5800",debitAmount:100},{accountNo:"5000",creditAmount:100}]};
      expect(dryRunJournalEntry(db,{...base,transactionDate:"2026-08-16"}).ok).toBe(false);expect(dryRunJournalEntry(db,{...base,lines:[{accountNo:"3000",debitAmount:100},{accountNo:"5000",creditAmount:100}]}).ok).toBe(false);expect(dryRunJournalEntry(db,{...base,lines:[{accountNo:"2000",debitAmount:100},{accountNo:"5000",creditAmount:100}]}).ok).toBe(false);expect(dryRunJournalEntry(db,{...base,lines:[{accountNo:"4000",debitAmount:100},{accountNo:"5000",creditAmount:100}]}).ok).toBe(false);expect(dryRunJournalEntry(db,{...base,lines:[{accountNo:"5800",debitAmount:99},{accountNo:"5000",creditAmount:99}]}).ok).toBe(false);
      db.run("INSERT INTO bank_accounts(slug,name,currency,ledger_account_no,active) VALUES('synthetic-bank','Synthetic bank','DKK','5800',1)");expect(dryRunJournalEntry(db,base).errors.join(" ")).toContain("not an eligible non-cash balance account");db.run("UPDATE bank_accounts SET active=0 WHERE slug='synthetic-bank'");expect(dryRunJournalEntry(db,base).errors.join(" ")).toContain("not an eligible non-cash balance account");
      const eligible={...base,lines:[{accountNo:"1300",debitAmount:100},{accountNo:"5000",creditAmount:100}]};expect(postJournalEntry(db,eligible).ok).toBe(true);expect(postJournalEntry(db,{...eligible,text:"conflict"}).errors.join(" ")).toContain("conflicting journal");db.close();
    }finally{rmSync(root,{recursive:true,force:true});rmSync(inbox,{recursive:true,force:true});}
  });
});
