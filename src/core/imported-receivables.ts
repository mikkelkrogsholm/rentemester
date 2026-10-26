import { canonicalJson } from "./canonical-json";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { isValidIsoDate } from "./dates";
import { toOre } from "./money";
import { insertAuditLog } from "./actor";
import type { StablePrincipal } from "./idempotency";
import { postJournalEntryInCurrentTransaction } from "./ledger";
import { resolveSettlementBankAccount } from "./invoice-fx-receivable";

export type ImportedReceivableSchedule = {
  contract: "rentemester-imported-receivables-v1";
  sourceDocumentHash: string;
  invoices: Array<{
    id: string; customerId?: string; customerName?: string; invoiceDate: string; dueDate?: string;
    grossAmount: number; controlAccountNo: string; recognitionRef: string; documentHash: string;
    payments?: Array<{ id: string; eventKind?: "payment" | "credit_note"; paymentDate: string; amount: number; paymentRef: string; documentHash: string }>;
  }>;
};

const canonical = canonicalJson;
const sha=(value:unknown)=>createHash("sha256").update(canonical(value)).digest("hex");
const text = (value: unknown) => typeof value === "string" ? value.trim() : "";
const hash = (value: unknown) => /^[a-f0-9]{64}$/i.test(text(value));
const amount = (value: unknown) => typeof value === "number" && Number.isFinite(value) && toOre(value) > 0n;

export type LegacyImportedReceivableBackfillInput = {
  dineroImportAttemptId: number;
  sourceRawSha256: string;
  canonicalInventorySha256: string;
  controlDate: string;
  controlAccountNo: string;
  artifactSha256: string;
  schedule: ImportedReceivableSchedule;
};

export type ApplyLegacyImportedReceivableBackfillInput = LegacyImportedReceivableBackfillInput & {
  planHash: string;
  idempotencyKey: string;
  actor?: string;
  principal?: StablePrincipal;
  confirm: boolean;
};

export type ImportedReceivableBankSettlementInput = {
  scheduleHash: string;
  externalInvoiceId: string;
  bankTransactionId: number;
  bankAccountNo?: string;
};

export type ApplyImportedReceivableBankSettlementInput = ImportedReceivableBankSettlementInput & {
  planHash: string;
  idempotencyKey: string;
  actor?: string;
  principal?: StablePrincipal;
  confirm: boolean;
};

/** Validate the explicitly supported schedule contract; never derive invoices from voucher text. */
export function validateImportedReceivableSchedule(input: unknown, controlDate?: string): { ok: true; schedule: ImportedReceivableSchedule; hash: string } | { ok: false; errors: string[] } {
  const schedule = input as ImportedReceivableSchedule;
  const errors: string[] = [];
  if (controlDate !== undefined && !isValidIsoDate(controlDate)) errors.push("imported receivable control date must be YYYY-MM-DD");
  if (!schedule || schedule.contract !== "rentemester-imported-receivables-v1") errors.push("imported receivable schedule must use contract rentemester-imported-receivables-v1");
  if (!hash(schedule?.sourceDocumentHash)) errors.push("imported receivable schedule needs a source document SHA-256");
  if (!Array.isArray(schedule?.invoices) || schedule.invoices.length === 0) errors.push("imported receivable schedule needs at least one invoice");
  const ids = new Set<string>();
  for (const invoice of schedule?.invoices ?? []) {
    const id = text(invoice?.id); if (!id || ids.has(id)) errors.push(`imported receivable invoice has missing or duplicate id '${id}'`); ids.add(id);
    if (!isValidIsoDate(invoice?.invoiceDate ?? "")) errors.push(`imported receivable ${id || "?"} has invalid invoice date`);
    if (controlDate !== undefined && isValidIsoDate(invoice?.invoiceDate ?? "") && invoice.invoiceDate > controlDate) errors.push(`imported receivable ${id || "?"} is after control date ${controlDate}`);
    if (invoice?.dueDate != null && (!isValidIsoDate(invoice.dueDate) || invoice.dueDate < invoice.invoiceDate)) errors.push(`imported receivable ${id || "?"} has invalid due date`);
    if (!amount(invoice?.grossAmount)) errors.push(`imported receivable ${id || "?"} needs a positive gross amount`);
    if (!text(invoice?.controlAccountNo) || !text(invoice?.recognitionRef) || !hash(invoice?.documentHash)) errors.push(`imported receivable ${id || "?"} lacks authoritative source evidence`);
    let paid = 0n; const paymentIds = new Set<string>();
    for (const payment of invoice?.payments ?? []) { const paymentId=text(payment?.id); const kind=payment?.eventKind??"payment"; if (!paymentId || paymentIds.has(paymentId)) errors.push(`imported receivable ${id || "?"} has missing or duplicate event id '${paymentId}'`); paymentIds.add(paymentId); if (!['payment','credit_note'].includes(kind) || !isValidIsoDate(payment?.paymentDate ?? "") || payment.paymentDate<invoice.invoiceDate || !amount(payment?.amount) || !text(payment?.paymentRef) || !hash(payment?.documentHash)) errors.push(`imported receivable ${id || "?"} event ${paymentId || "?"} lacks authoritative evidence`); if (controlDate !== undefined && isValidIsoDate(payment?.paymentDate ?? "") && payment.paymentDate > controlDate) errors.push(`imported receivable ${id || "?"} event ${paymentId || "?"} is after control date ${controlDate}`); if (amount(payment?.amount)) paid += toOre(payment.amount); }
    if (amount(invoice?.grossAmount) && paid > toOre(invoice.grossAmount)) errors.push(`imported receivable ${id || "?"} payments exceed the invoice amount`);
  }
  return errors.length ? { ok: false, errors } : { ok: true, schedule, hash: sha(schedule) };
}

/** Persist one immutable imported schedule. Replays are identical or fail closed. */
export function recordImportedReceivableSchedule(db: Database, attemptId: number, input: unknown, controlDate: string): { ok: boolean; errors: string[]; scheduleHash?: string } {
  const checked = validateImportedReceivableSchedule(input, controlDate); if (!checked.ok) return checked;
  const prior = db.query("SELECT schedule_hash,control_date FROM imported_receivable_boundaries WHERE dinero_import_attempt_id=? LIMIT 1").get(attemptId) as { schedule_hash:string; control_date:string } | null;
  if (prior) return prior.schedule_hash === checked.hash && prior.control_date === controlDate ? { ok:true, errors:[], scheduleHash: checked.hash } : { ok:false, errors:["imported receivable schedule conflicts with accepted source or control date"] };
  try { db.transaction(() => {
    db.query("INSERT INTO imported_receivable_boundaries(dinero_import_attempt_id,control_date,schedule_hash,source_document_hash) VALUES(?,?,?,?)").run(attemptId,controlDate,checked.hash,checked.schedule.sourceDocumentHash);
    const add = db.query("INSERT INTO imported_receivable_headers(dinero_import_attempt_id,external_invoice_id,source_document_hash,customer_external_id,customer_name,invoice_date,due_date,gross_amount,control_account_no,source_recognition_ref,schedule_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?) RETURNING id");
    const addPayment = db.query("INSERT INTO imported_receivable_events(receivable_id,external_event_id,event_kind,effective_date,amount,source_event_ref,source_document_hash,schedule_hash) VALUES(?,?,?,?,?,?,?,?)");
    for (const invoice of checked.schedule.invoices) {
      if (!db.query("SELECT id FROM accounts WHERE account_no=?").get(invoice.controlAccountNo)) throw new Error(`imported receivable ${invoice.id} has unknown control account`);
      const row = add.get(attemptId,invoice.id,invoice.documentHash,text(invoice.customerId)||null,text(invoice.customerName)||null,invoice.invoiceDate,invoice.dueDate ?? null,invoice.grossAmount,invoice.controlAccountNo,invoice.recognitionRef,checked.hash) as {id:number};
      for (const payment of invoice.payments ?? []) addPayment.run(row.id,payment.id,payment.eventKind??"payment",payment.paymentDate,payment.amount,payment.paymentRef,payment.documentHash,checked.hash);
    }
  }).immediate(); return {ok:true,errors:[],scheduleHash:checked.hash}; } catch (error) { return {ok:false,errors:[error instanceof Error ? error.message : String(error)]}; }
}

function activeLedgerBalanceOre(db: Database, accountNo: string, cutoff: string): bigint {
  const row = db.query(`SELECT COALESCE(SUM(jl.debit_amount-jl.credit_amount),0) AS balance
    FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id
    JOIN accounts a ON a.id=jl.account_id WHERE a.account_no=? AND je.transaction_date<=?`).get(accountNo,cutoff) as {balance:number};
  return toOre(Number(row.balance));
}

function ledgerHead(db: Database): string | null {
  return (db.query("SELECT entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1").get() as {entry_hash:string}|null)?.entry_hash ?? null;
}

function auditHead(db: Database): string {
  const row=db.query("SELECT id,event_type,entity_type,entity_id,message,actor,created_at FROM audit_log ORDER BY id DESC LIMIT 1").get() as Record<string,unknown>|null;
  return sha(row ?? {empty:true});
}

function legacyBackfillContext(db: Database, input: LegacyImportedReceivableBackfillInput) {
  const errors:string[]=[];
  if(!Number.isInteger(input.dineroImportAttemptId)||input.dineroImportAttemptId<=0)errors.push("DINERO_IMPORT_ATTEMPT_REQUIRED");
  if(!hash(input.sourceRawSha256)||!hash(input.canonicalInventorySha256)||!hash(input.artifactSha256))errors.push("SOURCE_HASHES_REQUIRED");
  if(!isValidIsoDate(input.controlDate))errors.push("CONTROL_DATE_INVALID");
  const account=text(input.controlAccountNo); if(!account)errors.push("CONTROL_ACCOUNT_REQUIRED");
  const checked=validateImportedReceivableSchedule(input.schedule,input.controlDate); if(!checked.ok)errors.push(...checked.errors);
  if(checked.ok&&checked.hash!==input.artifactSha256)errors.push("ARTIFACT_HASH_MISMATCH");
  const attempt=db.query(`SELECT a.id,a.outcome,a.parser_contract,a.source_raw_sha256,i.canonical_listing_sha256
    FROM dinero_import_attempts a JOIN dinero_import_inventories i ON i.id=a.inventory_id
    WHERE a.id=?`).get(input.dineroImportAttemptId) as {id:number;outcome:string;parser_contract:string;source_raw_sha256:string;canonical_listing_sha256:string}|null;
  if(!attempt||attempt.outcome!=="accepted"||!attempt.parser_contract.startsWith("dinero-"))errors.push("ACCEPTED_DINERO_IMPORT_NOT_FOUND");
  else {
    if(attempt.source_raw_sha256!==input.sourceRawSha256)errors.push("SOURCE_RAW_HASH_MISMATCH");
    if(attempt.canonical_listing_sha256!==input.canonicalInventorySha256)errors.push("INVENTORY_HASH_MISMATCH");
  }
  const role=db.query(`SELECT account_no FROM account_role_mappings WHERE role='debtors' AND status='confirmed' ORDER BY id DESC LIMIT 2`).all() as Array<{account_no:string}>;
  if(role.length!==1||role[0]!.account_no!==account)errors.push("DEBTORS_CONTROL_ACCOUNT_MISMATCH");
  if(checked.ok&&checked.schedule.invoices.some(invoice=>invoice.controlAccountNo!==account))errors.push("SCHEDULE_CONTROL_ACCOUNT_MISMATCH");
  const overlap=db.query(`SELECT d.id FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id
    JOIN journal_entries je ON je.id=p.journal_entry_id WHERE d.document_type='issued_invoice' AND d.invoice_date<=?
    AND p.receivable_account_id=(SELECT id FROM accounts WHERE account_no=?) LIMIT 1`).get(input.controlDate,account);
  if(overlap)errors.push("NATIVE_RECEIVABLE_OVERLAP");
  const ledgerBalance=account?activeLedgerBalanceOre(db,account,input.controlDate):0n;
  const scheduleBalance=checked.ok?importedScheduleBalanceOre(checked.schedule,input.controlDate,account):0n;
  if(checked.ok&&ledgerBalance!==scheduleBalance)errors.push(`CONTROL_BALANCE_MISMATCH:${ledgerBalance}:${scheduleBalance}`);
  const existing=db.query("SELECT plan_hash,schedule_hash,control_date,control_account_no FROM legacy_imported_receivable_backfills WHERE dinero_import_attempt_id=? LIMIT 1").get(input.dineroImportAttemptId) as {plan_hash:string;schedule_hash:string;control_date:string;control_account_no:string}|null;
  const state={
    contract:"rentemester-legacy-imported-receivable-backfill-plan-v1",
    dineroImportAttemptId:input.dineroImportAttemptId,
    sourceRawSha256:input.sourceRawSha256,
    canonicalInventorySha256:input.canonicalInventorySha256,
    controlDate:input.controlDate,
    controlAccountNo:account,
    artifactSha256:input.artifactSha256,
    scheduleHash:checked.ok?checked.hash:"",
    invoiceCount:checked.ok?checked.schedule.invoices.length:0,
    eventCount:checked.ok?checked.schedule.invoices.reduce((n,invoice)=>n+(invoice.payments?.length??0),0):0,
    ledgerBalanceOre:ledgerBalance.toString(),
    scheduleBalanceOre:scheduleBalance.toString(),
    ledgerHeadHash:ledgerHead(db),
    auditHeadHash:auditHead(db),
  };
  return {errors,checked,state,existing};
}

/** Read-only plan for adopting the canonical v36+ subledger after a legacy accepted import. */
export function planLegacyImportedReceivableBackfill(db: Database,input:LegacyImportedReceivableBackfillInput) {
  const context=legacyBackfillContext(db,input);
  if(context.errors.length)return {ok:false as const,errors:context.errors};
  if(context.existing)return {ok:true as const,alreadyApplied:true,plan:{...context.state,planHash:context.existing.plan_hash},errors:[] as string[]};
  return {ok:true as const,alreadyApplied:false,plan:{...context.state,planHash:sha(context.state)},errors:[] as string[]};
}

/** Atomically appends only schedule, boundary, backfill evidence and audit. */
export function applyLegacyImportedReceivableBackfill(db:Database,input:ApplyLegacyImportedReceivableBackfillInput) {
  if(!input.confirm)return {ok:false as const,errors:["CONFIRMATION_REQUIRED"]};
  const actor=text(input.actor),principalKind=input.principal?.kind,principalId=text(input.principal?.subjectId),key=text(input.idempotencyKey);
  if(!actor||!principalId||(principalKind!=="user"&&principalKind!=="service-account"))return {ok:false as const,errors:["ACTOR_AND_PRINCIPAL_REQUIRED"]};
  if(!key||key.length>128)return {ok:false as const,errors:["IDEMPOTENCY_KEY_REQUIRED"]};
  try{return db.transaction(()=>{
    const retry=db.query("SELECT id,plan_hash,dinero_import_attempt_id FROM legacy_imported_receivable_backfills WHERE principal_kind=? AND principal_subject_id=? AND idempotency_key=? LIMIT 1").get(principalKind,principalId,key) as {id:number;plan_hash:string;dinero_import_attempt_id:number}|null;
    if(retry)return retry.plan_hash===input.planHash&&retry.dinero_import_attempt_id===input.dineroImportAttemptId?{ok:true as const,id:retry.id,idempotent:true,planHash:retry.plan_hash,errors:[] as string[]}:{ok:false as const,errors:["IDEMPOTENCY_CONFLICT"]};
    const proposal=planLegacyImportedReceivableBackfill(db,input); if(!proposal.ok)return proposal;
    if(proposal.alreadyApplied)return proposal.plan.planHash===input.planHash?{ok:true as const,idempotent:true,planHash:input.planHash,errors:[] as string[]}:{ok:false as const,errors:["BACKFILL_CONFLICT"]};
    if(proposal.plan.planHash!==input.planHash)return {ok:false as const,errors:["PLAN_HASH_MISMATCH"]};
    const recorded=recordImportedReceivableSchedule(db,input.dineroImportAttemptId,input.schedule,input.controlDate); if(!recorded.ok)return recorded;
    const inserted=db.query(`INSERT INTO legacy_imported_receivable_backfills(dinero_import_attempt_id,control_date,control_account_no,source_raw_sha256,canonical_inventory_sha256,artifact_sha256,schedule_hash,plan_hash,ledger_head_hash,audit_head_hash,ledger_balance_ore,idempotency_key,actor,principal_kind,principal_subject_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(input.dineroImportAttemptId,input.controlDate,input.controlAccountNo,input.sourceRawSha256,input.canonicalInventorySha256,input.artifactSha256,recorded.scheduleHash!,input.planHash,proposal.plan.ledgerHeadHash,proposal.plan.auditHeadHash,proposal.plan.ledgerBalanceOre,key,actor,principalKind,principalId,new Date().toISOString()) as {id:number};
    insertAuditLog(db,{eventType:"legacy_imported_receivables_backfilled",entityType:"dinero_import_attempt",entityId:input.dineroImportAttemptId,message:`Appended verified receivable schedule ${recorded.scheduleHash} at ${input.controlDate} without replaying the import`,createdBy:actor,createdByProgram:"legacy-imported-receivables-backfill"});
    return {ok:true as const,id:inserted.id,idempotent:false,planHash:input.planHash,scheduleHash:recorded.scheduleHash,errors:[] as string[]};
  }).immediate();}catch(error){return {ok:false as const,errors:[error instanceof Error?error.message:String(error)]};}
}

type ImportedReceivableSettlementContext = {
  errors: string[];
  state: Record<string, unknown>;
  header?: { id: number; control_account_no: string; schedule_hash: string; invoice_date: string; gross_amount: number };
  bank?: { id: number; transaction_date: string; amount: number; currency: string | null; transaction_hash: string };
  openAmount?: number;
};

function importedReceivableSettlementContext(
  db: Database,
  input: ImportedReceivableBankSettlementInput,
): ImportedReceivableSettlementContext {
  const errors: string[] = [];
  const scheduleHash = text(input.scheduleHash).toLowerCase();
  const externalInvoiceId = text(input.externalInvoiceId);
  if (!hash(scheduleHash) || !externalInvoiceId) errors.push("RECEIVABLE_IDENTITY_REQUIRED");
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0) errors.push("BANK_TRANSACTION_REQUIRED");

  const header = db.query(`SELECT id, control_account_no, schedule_hash, invoice_date, gross_amount
      FROM imported_receivable_headers WHERE schedule_hash=? AND external_invoice_id=? LIMIT 2`)
    .all(scheduleHash, externalInvoiceId) as Array<{ id: number; control_account_no: string; schedule_hash: string; invoice_date: string; gross_amount: number }>;
  if (header.length !== 1) errors.push("IMPORTED_RECEIVABLE_NOT_UNAMBIGUOUS");
  const receivable = header[0];
  const bank = db.query(`SELECT id, transaction_date, amount, currency, transaction_hash
      FROM bank_transactions WHERE id=?`).get(input.bankTransactionId) as { id: number; transaction_date: string; amount: number; currency: string | null; transaction_hash: string } | null;
  if (!bank) errors.push("BANK_TRANSACTION_NOT_FOUND");
  if (bank && ((bank.currency ?? "DKK").trim().toUpperCase() !== "DKK")) errors.push("IMPORTED_RECEIVABLE_CURRENCY_MISMATCH");
  if (bank && (!(Number(bank.amount) > 0) || !hash(bank.transaction_hash))) errors.push("BANK_TRANSACTION_NOT_SETTLEABLE");
  if (bank && db.query("SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=? LIMIT 1").get(bank.id)) errors.push("BANK_TRANSACTION_ALREADY_RECONCILED");

  let openAmount = 0;
  if (receivable && bank) {
    const paid = db.query("SELECT COALESCE(SUM(amount),0) AS amount FROM imported_receivable_events WHERE receivable_id=?").get(receivable.id) as { amount: number };
    openAmount = Number(receivable.gross_amount) - Number(paid.amount);
    if (bank.transaction_date < receivable.invoice_date) errors.push("SETTLEMENT_BEFORE_RECEIVABLE");
    if (!(openAmount > 0)) errors.push("IMPORTED_RECEIVABLE_ALREADY_SETTLED");
    if (Number(bank.amount) > openAmount) errors.push("IMPORTED_RECEIVABLE_OVERPAYMENT");
    const account = resolveSettlementBankAccount(db, { bankTransactionId: bank.id, requestedAccountNo: input.bankAccountNo });
    if (!account.ok) errors.push(`BANK_ACCOUNT_INVALID:${account.error}`);
  }

  const state = {
    contract: "rentemester-imported-receivable-bank-settlement-plan-v1",
    scheduleHash,
    externalInvoiceId,
    receivableId: receivable?.id ?? null,
    controlAccountNo: receivable?.control_account_no ?? null,
    invoiceDate: receivable?.invoice_date ?? null,
    openAmount,
    bankTransactionId: bank?.id ?? input.bankTransactionId,
    bankTransactionHash: bank?.transaction_hash ?? null,
    bankTransactionDate: bank?.transaction_date ?? null,
    amount: bank?.amount ?? null,
    currency: bank?.currency ?? "DKK",
    ledgerHeadHash: ledgerHead(db),
    auditHeadHash: auditHead(db),
  };
  return { errors, state, header: receivable, bank: bank ?? undefined, openAmount };
}

/** Read-only, hash-bound plan for settling one imported DKK receivable from one bank receipt. */
export function planImportedReceivableBankSettlement(db: Database, input: ImportedReceivableBankSettlementInput) {
  const context = importedReceivableSettlementContext(db, input);
  if (context.errors.length) return { ok: false as const, errors: context.errors };
  return { ok: true as const, plan: { ...context.state, planHash: sha(context.state) }, errors: [] as string[] };
}

/** Atomically posts the bank/control movement and appends its imported payment evidence. */
export function applyImportedReceivableBankSettlement(db: Database, input: ApplyImportedReceivableBankSettlementInput) {
  if (!input.confirm) return { ok: false as const, errors: ["CONFIRMATION_REQUIRED"] };
  const actor = text(input.actor);
  const principalKind = input.principal?.kind;
  const principalId = text(input.principal?.subjectId);
  const key = text(input.idempotencyKey);
  if (!actor || !principalId || (principalKind !== "user" && principalKind !== "service-account")) return { ok: false as const, errors: ["ACTOR_AND_PRINCIPAL_REQUIRED"] };
  if (!key || key.length > 128) return { ok: false as const, errors: ["IDEMPOTENCY_KEY_REQUIRED"] };
  try {
    return db.transaction(() => {
      const prior = db.query(`SELECT id, plan_hash, journal_entry_id FROM imported_receivable_bank_settlements
        WHERE principal_kind=? AND principal_subject_id=? AND idempotency_key=? LIMIT 1`)
        .get(principalKind, principalId, key) as { id: number; plan_hash: string; journal_entry_id: number } | null;
      if (prior) return prior.plan_hash === input.planHash
        ? { ok: true as const, id: prior.id, journalEntryId: prior.journal_entry_id, planHash: prior.plan_hash, idempotent: true, errors: [] as string[] }
        : { ok: false as const, errors: ["IDEMPOTENCY_CONFLICT"] };

      const proposal = planImportedReceivableBankSettlement(db, input);
      if (!proposal.ok) return proposal;
      if (proposal.plan.planHash !== input.planHash) return { ok: false as const, errors: ["PLAN_HASH_MISMATCH"] };
      const context = importedReceivableSettlementContext(db, input);
      if (context.errors.length || !context.header || !context.bank) return { ok: false as const, errors: context.errors.length ? context.errors : ["SETTLEMENT_CONTEXT_UNAVAILABLE"] };
      const bankAccount = resolveSettlementBankAccount(db, { bankTransactionId: context.bank.id, requestedAccountNo: input.bankAccountNo });
      if (!bankAccount.ok) return { ok: false as const, errors: [`BANK_ACCOUNT_INVALID:${bankAccount.error}`] };
      const journal = postJournalEntryInCurrentTransaction(db, {
        transactionDate: context.bank.transaction_date,
        text: `Imported receivable settlement ${input.externalInvoiceId}`,
        sourceBankTransactionId: context.bank.id,
        createdBy: actor,
        createdByProgram: "imported-receivable-bank-settlement",
        lines: [
          { accountNo: bankAccount.accountNo, debitAmount: context.bank.amount, text: `Bank receipt ${context.bank.id}` },
          { accountNo: context.header.control_account_no, creditAmount: context.bank.amount, text: `Imported receivable ${input.externalInvoiceId}` },
        ],
      });
      if (!journal.ok || journal.entryId == null) return { ok: false as const, errors: journal.errors, appliedRules: journal.appliedRules };
      const eventId = `bank-settlement:${context.bank.transaction_hash}`;
      db.query(`INSERT INTO imported_receivable_events(receivable_id,external_event_id,event_kind,effective_date,amount,source_event_ref,source_document_hash,schedule_hash)
        VALUES(?,?,?,?,?,?,?,?)`).run(context.header.id, eventId, "payment", context.bank.transaction_date, context.bank.amount, `bank_transaction:${context.bank.id}`, context.bank.transaction_hash, context.header.schedule_hash);
      const inserted = db.query(`INSERT INTO imported_receivable_bank_settlements(receivable_id,bank_transaction_id,bank_transaction_hash,journal_entry_id,schedule_hash,plan_hash,amount,effective_date,actor,principal_kind,principal_subject_id,idempotency_key,created_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(context.header.id, context.bank.id, context.bank.transaction_hash, journal.entryId, context.header.schedule_hash, input.planHash, context.bank.amount, context.bank.transaction_date, actor, principalKind, principalId, key, new Date().toISOString()) as { id: number };
      insertAuditLog(db, { eventType: "imported_receivable_bank_settled", entityType: "imported_receivable_bank_settlement", entityId: inserted.id, message: `Settled imported receivable ${input.externalInvoiceId} from bank transaction ${context.bank.id}`, createdBy: actor, createdByProgram: "imported-receivable-bank-settlement" });
      return { ok: true as const, id: inserted.id, journalEntryId: journal.entryId, planHash: input.planHash, idempotent: false, openBalance: Number(context.openAmount) - Number(context.bank.amount), errors: [] as string[], appliedRules: journal.appliedRules };
    }).immediate();
  } catch (error) {
    return { ok: false as const, errors: [error instanceof Error ? error.message : String(error)] };
  }
}

/** Read back an immutable settlement by its selected bank transaction. */
export function getImportedReceivableBankSettlement(db: Database, bankTransactionId: number) {
  if (!Number.isInteger(bankTransactionId) || bankTransactionId <= 0) return { ok: false as const, errors: ["BANK_TRANSACTION_REQUIRED"] };
  const row = db.query(`SELECT s.id,s.bank_transaction_id,s.bank_transaction_hash,s.journal_entry_id,s.schedule_hash,s.plan_hash,s.amount,s.effective_date,s.actor,s.created_at,h.external_invoice_id
    FROM imported_receivable_bank_settlements s JOIN imported_receivable_headers h ON h.id=s.receivable_id WHERE s.bank_transaction_id=?`).get(bankTransactionId) as Record<string, unknown> | null;
  return { ok: true as const, settlement: row ?? null, errors: [] as string[] };
}

/** One authoritative cut-over boundary per imported receivable control. */
export function importedReceivableControlDate(db: Database, controlAccountNo: string): string | null {
  const rows = db.query(`SELECT DISTINCT b.control_date
    FROM imported_receivable_boundaries b
    JOIN imported_receivable_headers h ON h.dinero_import_attempt_id=b.dinero_import_attempt_id
    WHERE h.control_account_no=? ORDER BY b.control_date`).all(controlAccountNo) as Array<{ control_date: string }>;
  if (rows.length > 1) throw new Error(`imported receivable control ${controlAccountNo} has conflicting cut-over boundaries`);
  return rows[0]?.control_date ?? null;
}

/** Exact imported source balance at a date, including paid and fully-settled invoices. */
export function importedReceivableBalanceOre(db: Database, cutoff: string, controlAccountNo: string): { total: bigint; evidence: Array<Record<string, unknown>> } {
  const records = db.query(`SELECT h.id,h.external_invoice_id,h.customer_external_id,h.customer_name,h.invoice_date,h.due_date,h.gross_amount,h.source_document_hash,h.schedule_hash,COALESCE(SUM(CASE WHEN p.effective_date<=? THEN p.amount ELSE 0 END),0) paid_amount FROM imported_receivable_headers h LEFT JOIN imported_receivable_events p ON p.receivable_id=h.id WHERE h.invoice_date<=? AND h.control_account_no=? GROUP BY h.id ORDER BY h.id`).all(cutoff,cutoff,controlAccountNo) as Array<Record<string,unknown>>;
  return { total: records.reduce((sum,row)=>sum+toOre(Number(row.gross_amount))-toOre(Number(row.paid_amount)),0n), evidence: records.map(row=>({source:"imported-receivable",externalInvoiceId:row.external_invoice_id,customerExternalId:row.customer_external_id,customerName:row.customer_name,invoiceDate:row.invoice_date,dueDate:row.due_date,grossDkk:row.gross_amount,paidDkk:row.paid_amount,sourceDocumentHash:row.source_document_hash,scheduleHash:row.schedule_hash})) };
}

export function importedScheduleBalanceOre(schedule:ImportedReceivableSchedule,cutoff:string,controlAccountNo:string):bigint {
  return schedule.invoices.filter(invoice=>invoice.controlAccountNo===controlAccountNo&&invoice.invoiceDate<=cutoff).reduce((sum,invoice)=>sum+toOre(invoice.grossAmount)-(invoice.payments??[]).filter(event=>event.paymentDate<=cutoff).reduce((paid,event)=>paid+toOre(event.amount),0n),0n);
}

/** Read-only canonical imported receivable list. Imported rows remain source
 * records, never masquerade as Rentemester-issued invoices, and explicitly
 * expose the archive/cut-over boundary to callers. */
export function listImportedReceivables(db: Database, asOfDate: string): { ok: boolean; asOfDate: string; boundary: string; count: number; totalOpen: number; rows: Array<Record<string, unknown>>; errors: string[] } {
  if (!isValidIsoDate(asOfDate)) return { ok:false, asOfDate, boundary:"imported source records only; native invoices are listed separately", count:0,totalOpen:0,rows:[],errors:["as-of date must be YYYY-MM-DD"] };
  const rows = db.query(`SELECT h.external_invoice_id,h.customer_external_id,h.customer_name,h.invoice_date,h.due_date,h.gross_amount,h.control_account_no,h.source_recognition_ref,h.source_document_hash,h.schedule_hash,COALESCE(SUM(CASE WHEN p.effective_date<=? THEN p.amount ELSE 0 END),0) paid_amount FROM imported_receivable_headers h LEFT JOIN imported_receivable_events p ON p.receivable_id=h.id WHERE h.invoice_date<=? GROUP BY h.id ORDER BY h.invoice_date,h.id`).all(asOfDate,asOfDate) as Array<Record<string,unknown>>;
  const result = rows.map(row => ({ source:"imported" as const, externalInvoiceId:row.external_invoice_id, customerExternalId:row.customer_external_id, customerName:row.customer_name, invoiceDate:row.invoice_date, dueDate:row.due_date, grossAmount:Number(row.gross_amount), paidAmount:Number(row.paid_amount), openBalance:Number(row.gross_amount)-Number(row.paid_amount), controlAccountNo:row.control_account_no, sourceRecognitionRef:row.source_recognition_ref, sourceDocumentHash:row.source_document_hash, scheduleHash:row.schedule_hash, archiveBoundary:"Imported source record; use invoice list for Rentemester-issued invoices." }));
  const boundaries = db.query("SELECT DISTINCT control_date FROM imported_receivable_boundaries ORDER BY control_date").all() as Array<{control_date:string}>;
  const boundary = boundaries.length === 1
    ? `Imported source records through ${boundaries[0]!.control_date}; native Rentemester invoices are supported only after this cut-over.`
    : "Imported source records only; native Rentemester invoices are deliberately separate to avoid duplicate claims.";
  return { ok:true,asOfDate,boundary,count:result.length,totalOpen:result.reduce((sum,row)=>sum+row.openBalance,0),rows:result,errors:[] };
}
