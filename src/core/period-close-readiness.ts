import { canonicalJson } from "./canonical-json";
/** Pure period-close readiness plus explicit, append-only review evidence. */
import { createHash } from "node:crypto";
import type { Database, SQLQueryBindings } from "bun:sqlite";
import { verifyAuditChain } from "./ledger";
import { verifyAuditLogIntegrity } from "./audit-log";
import { buildTrialBalance } from "./financial-statements";
import { buildVatReport } from "./vat";
import { resolveAccountRole } from "./account-roles";
import { fromOre, toOre } from "./money";
import { importedReceivableBalanceOre, importedReceivableControlDate } from "./imported-receivables";
import { purchaseCaseSourceFingerprint, type PurchaseCaseSource } from "./purchase-cases";

export type CloseControlStatus = "passed" | "warning" | "blocked" | "unavailable";
export type CloseReadinessItem = { code: string; status: CloseControlStatus; waivable: boolean; count: number; amount: number; evidence: readonly Record<string, unknown>[]; sourceHash: string };
export type CloseReadinessPacket = { version: 4; periodStart: string; periodEnd: string; cutoff: string; controlsRun: readonly string[]; items: readonly CloseReadinessItem[]; blockers: number; warnings: number; hash: string };
export type CloseReviewPrincipal = { kind: "user" | "service-account" | "local-trusted"; subjectId: string };
export type PeriodCloseReview = { id: number; packet: CloseReadinessPacket; reviewerActor: string; reviewerPrincipal: CloseReviewPrincipal | null; createdAt: string };

export const canonicalCloseReadiness = canonicalJson;
export function closeReadinessDigest(value: unknown): string { return createHash("sha256").update(canonicalCloseReadiness(value)).digest("hex"); }
function exists(db: Database, name: string): boolean { return db.query("SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name=?").get(name) !== null; }
function has(db: Database, name: string, column: string): boolean { return exists(db,name) && (db.query(`PRAGMA table_info(${name})`).all() as Array<{name:string}>).some(row => row.name === column); }
function rows(db: Database, sql: string, ...params: SQLQueryBindings[]): Array<Record<string, unknown>> { return db.query(sql).all(...params) as Array<Record<string, unknown>>; }
function control(code: string, status: CloseControlStatus, waivable: boolean, evidence: readonly Record<string, unknown>[], amount = 0): CloseReadinessItem { const ordered=[...evidence].sort((a,b)=>canonicalCloseReadiness(a).localeCompare(canonicalCloseReadiness(b))); return {code,status,waivable,count:ordered.length,amount,evidence:ordered,sourceHash:closeReadinessDigest(ordered)}; }
function unavailable(code: string, detail: string): CloseReadinessItem { return control(code,"unavailable",false,[{detail}]); }
function protect(code: string, run: () => CloseReadinessItem): CloseReadinessItem { try { return run(); } catch { return unavailable(code,"control execution failed"); } }
export function periodCloseReviewSchemaAvailable(db: Database): boolean { return exists(db,"period_close_readiness_packets") && exists(db,"period_close_reviews"); }

type DkkControlSource = { role: "bank" | "debtors" | "creditors"; accountNo: string; ledgerOre: bigint; sourceOre: bigint; evidence: Record<string, unknown>[] };

function dkkControlSchemaAvailable(db: Database): boolean {
  return [
    ["journal_entries", "transaction_date"], ["journal_entries", "status"], ["journal_lines", "account_id"],
    ["accounts", "account_no"], ["bank_transactions", "transaction_date"], ["bank_transactions", "balance_after"],
    ["documents", "document_type"], ["documents", "invoice_date"], ["issued_invoice_postings", "invoice_document_id"],
    ["issued_invoice_postings", "booked_gross_dkk"], ["invoice_payments", "invoice_document_id"], ["invoice_payments", "payment_date"],
    ["payables", "bill_date"], ["payables", "gross_amount"], ["payable_payments", "payable_id"], ["payable_payments", "payment_date"],
  ].every(([table, column]) => has(db, table, column));
}

function activeLedgerBalanceOre(db: Database, accountNo: string, cutoff: string): bigint {
  // Reversals are append-only counter-postings. Including both original and
  // reversal is therefore the canonical balance and avoids treating either as
  // a second live open item.
  const row = db.query(`SELECT COALESCE(SUM(jl.debit_amount - jl.credit_amount), 0) AS balance
    FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id JOIN accounts a ON a.id=jl.account_id
    WHERE a.account_no=? AND je.transaction_date<=?`).get(accountNo, cutoff) as { balance: number };
  return toOre(Number(row.balance));
}

function uniqueStatementEndpoints(db: Database, cutoff: string, accountNo: string): { ok: true; totalOre: bigint; evidence: Record<string, unknown>[] } | { ok: false; detail: string } {
  const configured = rows(db, `SELECT id FROM bank_accounts WHERE active=1 AND currency='DKK' AND ledger_account_no=? ORDER BY id`, accountNo) as Array<{ id: number }>;
  const streams = configured.length
    ? configured.map(row => ({ key: `bank-account:${row.id}`, id: row.id }))
    : [{ key: "legacy-unassigned", id: null as number | null }];
  let total = 0n;
  const evidence: Record<string, unknown>[] = [];
  for (const stream of streams) {
    const candidates = rows(db, `SELECT id,transaction_date,amount,balance_after FROM bank_transactions
      WHERE transaction_date<=? AND currency='DKK' AND balance_after IS NOT NULL
      AND bank_account_id IS ?
      AND transaction_date=(SELECT MAX(transaction_date) FROM bank_transactions b2
        WHERE b2.transaction_date<=? AND b2.currency='DKK' AND b2.balance_after IS NOT NULL AND b2.bank_account_id IS ?)
      ORDER BY id`, cutoff, stream.id, cutoff, stream.id) as Array<{ id: number; transaction_date: string; amount: number; balance_after: number }>;
    const anyImported = db.query(`SELECT 1 FROM bank_transactions WHERE transaction_date<=? AND currency='DKK' AND bank_account_id IS ? LIMIT 1`).get(cutoff, stream.id);
    if (!anyImported) {
      // Once a specific bank account has been mapped, a missing statement is
      // not evidence of a zero balance. The only harmless empty case is a
      // completely unconfigured, zero-activity legacy ledger.
      if (configured.length) return { ok: false, detail: `${stream.key} has no imported DKK statement at or before cutoff` };
      continue;
    }
    if (!candidates.length) return { ok: false, detail: `${stream.key} has no DKK balance_after at or before cutoff` };
    const before = new Set(candidates.map(row => toOre(Number(row.balance_after) - Number(row.amount)).toString()));
    const endpoints = candidates.filter(row => !before.has(toOre(Number(row.balance_after)).toString()));
    if (endpoints.length !== 1) return { ok: false, detail: `${stream.key} has ambiguous latest DKK statement balance` };
    const endpoint = endpoints[0]!;
    total += toOre(Number(endpoint.balance_after));
    evidence.push({ role: "bank", stream: stream.key, statementTransactionId: endpoint.id, statementDate: endpoint.transaction_date, statementBalanceDkk: fromOre(toOre(Number(endpoint.balance_after))), sourceCount: candidates.length });
  }
  return { ok: true, totalOre: total, evidence };
}

function dkkControlAccounts(db: Database, cutoff: string): CloseReadinessItem {
  if (!dkkControlSchemaAvailable(db)) return unavailable("DKK_CONTROL_ACCOUNTS", "independent DKK control-account schema unavailable");
  const resolved = [resolveAccountRole(db,"bank"), resolveAccountRole(db,"debtors"), resolveAccountRole(db,"creditors")];
  if (resolved.some(role => !role.ok)) return unavailable("DKK_CONTROL_ACCOUNTS", resolved.filter(role=>!role.ok).map(role=>!role.ok ? role.error : "").join("; "));
  const roles = resolved as Array<Extract<typeof resolved[number], { ok: true }>>;
  for (const role of roles) {
    const count = db.query("SELECT COUNT(DISTINCT account_no) AS n FROM account_role_mappings WHERE role=? AND status='confirmed'").get(role.role) as { n: number };
    if (count.n !== 1) return unavailable("DKK_CONTROL_ACCOUNTS", `role '${role.role}' has ambiguous confirmed account mappings`);
  }
  const bank = roles.find(role => role.role === "bank")!;
  const debtors = roles.find(role => role.role === "debtors")!;
  const creditors = roles.find(role => role.role === "creditors")!;
  const ledger = new Map(roles.map(role => [role.role, activeLedgerBalanceOre(db, role.accountNo, cutoff)]));
  const bankSource = uniqueStatementEndpoints(db, cutoff, bank.accountNo);
  const anyBank = db.query("SELECT 1 FROM bank_transactions WHERE transaction_date<=? AND currency='DKK' LIMIT 1").get(cutoff);
  if (!bankSource.ok && anyBank) return unavailable("DKK_CONTROL_ACCOUNTS", bankSource.detail);
  const bankOre = bankSource.ok ? bankSource.totalOre : 0n;

  const foreignPayable = db.query("SELECT id,currency FROM payables WHERE bill_date<=? AND UPPER(currency)!='DKK' LIMIT 1").get(cutoff) as { id:number; currency:string } | null;
  if (foreignPayable) return unavailable("DKK_CONTROL_ACCOUNTS", `payable ${foreignPayable.id} uses ${foreignPayable.currency}; DKK payable control cannot value it independently`);
  const importedBoundary = importedReceivableControlDate(db, debtors.accountNo);
  if (importedBoundary) {
    const overlap = db.query(`SELECT d.id FROM documents d
      JOIN issued_invoice_postings p ON p.invoice_document_id=d.id
      JOIN journal_entries j ON j.id=p.journal_entry_id
      WHERE d.document_type='issued_invoice' AND d.invoice_date<=? AND j.status='posted'
        AND j.reversal_of_entry_id IS NULL
        AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)
        AND p.receivable_account_id=(SELECT id FROM accounts WHERE account_no=?) LIMIT 1`).get(importedBoundary, debtors.accountNo) as {id:number} | null;
    if (overlap) throw new Error(`native receivable ${overlap.id} overlaps imported cut-over boundary ${importedBoundary}`);
  }
  const receivableRows = rows(db, `SELECT d.id AS document_id, p.journal_entry_id, p.booked_gross_dkk,
      COALESCE(SUM(CASE WHEN ip.payment_date<=? AND ipj.status='posted' AND ipj.reversal_of_entry_id IS NULL
        AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=ipj.id) THEN ip.amount ELSE 0 END),0) AS paid_amount
    FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id
    JOIN journal_entries j ON j.id=p.journal_entry_id
    LEFT JOIN invoice_payments ip ON ip.invoice_document_id=d.id LEFT JOIN journal_entries ipj ON ipj.id=ip.journal_entry_id
    WHERE d.document_type='issued_invoice' AND d.invoice_date<=? AND j.transaction_date<=? AND j.status='posted' AND j.reversal_of_entry_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)
      AND p.receivable_account_id=(SELECT id FROM accounts WHERE account_no=?)
      AND (? IS NULL OR d.invoice_date>?)
    GROUP BY d.id,p.journal_entry_id,p.booked_gross_dkk ORDER BY d.id`, cutoff, cutoff, cutoff, debtors.accountNo, importedBoundary, importedBoundary);
  const payableRows = rows(db, `SELECT p.id AS payable_id,p.journal_entry_id,p.gross_amount,
      COALESCE(SUM(CASE WHEN pp.payment_date<=? AND ppj.status='posted' AND ppj.reversal_of_entry_id IS NULL
        AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=ppj.id) THEN pp.amount ELSE 0 END),0) AS paid_amount
    FROM payables p JOIN journal_entries j ON j.id=p.journal_entry_id
    LEFT JOIN payable_payments pp ON pp.payable_id=p.id LEFT JOIN journal_entries ppj ON ppj.id=pp.journal_entry_id
    WHERE p.bill_date<=? AND j.transaction_date<=? AND j.status='posted' AND j.reversal_of_entry_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=j.id)
      AND UPPER(p.currency)='DKK'
    GROUP BY p.id,p.journal_entry_id,p.gross_amount ORDER BY p.id`, cutoff, cutoff, cutoff);
  const receivableOre = receivableRows.reduce((sum,row)=>sum + toOre(Number(row.booked_gross_dkk)) - toOre(Number(row.paid_amount)),0n);
  const importedReceivables = importedReceivableBalanceOre(db, cutoff, debtors.accountNo);
  const payableOpenOre = payableRows.reduce((sum,row)=>sum + toOre(Number(row.gross_amount)) - toOre(Number(row.paid_amount)),0n);
  const sources: DkkControlSource[] = [
    { role:"bank", accountNo:bank.accountNo, ledgerOre:ledger.get("bank")!, sourceOre:bankOre, evidence:bankSource.ok ? bankSource.evidence : [{role:"bank",sourceCount:0,statementBalanceDkk:0}] },
    { role:"debtors", accountNo:debtors.accountNo, ledgerOre:ledger.get("debtors")!, sourceOre:receivableOre + importedReceivables.total, evidence:[...receivableRows.map(row=>({role:"debtors",source:"native",documentId:row.document_id,journalEntryId:row.journal_entry_id,grossDkk:row.booked_gross_dkk,paidDkk:row.paid_amount,importedCutoverBoundary:importedBoundary})), ...importedReceivables.evidence.map(row=>({role:"debtors",importedCutoverBoundary:importedBoundary,...row}))] },
    { role:"creditors", accountNo:creditors.accountNo, ledgerOre:ledger.get("creditors")!, sourceOre:-payableOpenOre, evidence:payableRows.map(row=>({role:"creditors",payableId:row.payable_id,journalEntryId:row.journal_entry_id,grossDkk:row.gross_amount,paidDkk:row.paid_amount})) },
  ];
  const evidence = sources.map(source => ({ role:source.role, accountNo:source.accountNo, ledgerBalanceDkk:fromOre(source.ledgerOre), sourceBalanceDkk:fromOre(source.sourceOre), differenceDkk:fromOre(source.ledgerOre-source.sourceOre), sourceCount:source.evidence.length, sourceIds:source.evidence }));
  const mismatches = sources.filter(source => source.ledgerOre !== source.sourceOre);
  return control("DKK_CONTROL_ACCOUNTS", mismatches.length ? "blocked" : "passed", false, evidence, Number(fromOre(mismatches.reduce((sum, source)=>sum + (source.ledgerOre-source.sourceOre < 0n ? source.sourceOre-source.ledgerOre : source.ledgerOre-source.sourceOre),0n))));
}

/** Read-only: no `migrate`, DDL, packet persistence, audit write or WAL write. */
export function computePeriodCloseReadiness(db: Database, input: { periodStart: string; periodEnd: string; cutoff?: string; companyRoot?: string }): CloseReadinessPacket {
  const cutoff=input.cutoff ?? input.periodEnd;
  const items: CloseReadinessItem[]=[];
  // A readiness packet can be inspected against an older ledger, but it can
  // never be silently treated as closable: the review/decision evidence is a
  // mandatory v25 contract.
  if (!periodCloseReviewSchemaAvailable(db)) items.push(unavailable("PERIOD_CLOSE_REVIEW_SCHEMA","period-close review schema v25 is unavailable; migrate before review or close"));
  items.push(protect("PERIOD_LIFECYCLE",()=>{
    if (!has(db,"accounting_periods","period_start") || !has(db,"accounting_periods","period_end") || !has(db,"accounting_periods","status")) return unavailable("PERIOD_LIFECYCLE","accounting period schema unavailable");
    const all=rows(db,`SELECT p.id,p.period_start,p.period_end,p.kind,p.status,p.reference,
      (SELECT a.event_type FROM audit_log a WHERE a.entity_type='accounting_period' AND a.entity_id=CAST(p.id AS TEXT) ORDER BY a.id DESC LIMIT 1) AS lifecycle_event
      FROM accounting_periods p WHERE NOT (p.period_end < ? OR p.period_start > ?) ORDER BY p.id`,input.periodStart,cutoff);
    // A reopen is append-only: the immutable period row remains closed, while
    // its last lifecycle event makes the effective state open again.
    const evidence=all.filter(row=>row.lifecycle_event!=="period_reopen");
    return control("PERIOD_LIFECYCLE",evidence.length?"blocked":"passed",false,evidence);
  }));
  items.push(protect("BANK_UNRECONCILED",()=>{
    if (!has(db,"bank_transactions","transaction_date") || !has(db,"bank_transactions","amount") || !has(db,"bank_journal_reconciliations","bank_transaction_id") || !has(db,"bank_journal_reconciliations","journal_entry_id")) return unavailable("BANK_UNRECONCILED","required bank reconciliation schema unavailable");
    const evidence=rows(db,`SELECT bt.id,bt.transaction_date,bt.amount,bt.currency FROM bank_transactions bt LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id WHERE br.journal_entry_id IS NULL AND (bt.transaction_date BETWEEN ? AND ? OR bt.transaction_date IS NULL) ORDER BY bt.transaction_date,bt.id`,input.periodStart,cutoff); return control("BANK_UNRECONCILED",evidence.length?"blocked":"passed",true,evidence,evidence.reduce((n,r)=>n+Math.abs(Number(r.amount??0)),0));
  }));
  items.push(protect("EXCEPTIONS_OPEN",()=>{
    if (!has(db,"exceptions","status") || !has(db,"exceptions","severity")) return unavailable("EXCEPTIONS_OPEN","required exception schema unavailable");
    if (!has(db,"exceptions","related_bank_transaction_id") || !has(db,"exceptions","related_document_id") || !has(db,"documents","invoice_date") || !has(db,"bank_transactions","transaction_date")) return unavailable("EXCEPTIONS_OPEN","exception date scope cannot be determined");
    const evidence=rows(db,`SELECT e.id,e.type,e.severity,bt.transaction_date,d.invoice_date FROM exceptions e LEFT JOIN bank_transactions bt ON bt.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE e.status='open' AND e.severity IN ('high','medium') AND ((bt.transaction_date BETWEEN ? AND ?) OR (d.invoice_date BETWEEN ? AND ?)) ORDER BY e.id`,input.periodStart,cutoff,input.periodStart,cutoff); return control("EXCEPTIONS_OPEN",evidence.length?"blocked":"passed",true,evidence);
  }));
  items.push(protect("EXCEPTION_SCOPE_UNKNOWN",()=>{
    if (!has(db,"exceptions","status") || !has(db,"exceptions","severity") || !has(db,"exceptions","related_bank_transaction_id") || !has(db,"exceptions","related_document_id") || !has(db,"documents","invoice_date") || !has(db,"bank_transactions","transaction_date")) return unavailable("EXCEPTION_SCOPE_UNKNOWN","exception scope schema unavailable");
    const evidence=rows(db,`SELECT e.id,e.type,e.severity FROM exceptions e LEFT JOIN bank_transactions bt ON bt.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE e.status='open' AND e.severity IN ('high','medium') AND bt.transaction_date IS NULL AND d.invoice_date IS NULL ORDER BY e.id`); return control("EXCEPTION_SCOPE_UNKNOWN",evidence.length?"blocked":"passed",false,evidence);
  }));
  items.push(protect("BATCH_UNPOSTED_OR_FAILED",()=>{
    if (!has(db,"bookkeeping_batch_runs","accounting_from") || !has(db,"bookkeeping_batch_revisions","run_id") || !has(db,"bookkeeping_batch_apply_attempts_v2","revision_id") || !has(db,"bookkeeping_batch_apply_events_v2","event_type") || !has(db,"bookkeeping_batch_final_checks_v2","ok")) return unavailable("BATCH_UNPOSTED_OR_FAILED","durable batch revision/apply schema unavailable");
    const evidence=rows(db,`SELECT r.id AS run_id,rev.id AS revision_id,a.id AS attempt_id,
      (SELECT e.event_type FROM bookkeeping_batch_apply_events_v2 e WHERE e.apply_attempt_id=a.id ORDER BY e.id DESC LIMIT 1) AS final_event,
      (SELECT COUNT(*) FROM bookkeeping_batch_apply_events_v2 e WHERE e.apply_attempt_id=a.id AND e.event_type IN ('item_failed','source_stale')) AS failed_events,
      (SELECT COUNT(*) FROM bookkeeping_batch_final_checks_v2 c WHERE c.apply_attempt_id=a.id AND c.ok=0) AS failed_checks
      FROM bookkeeping_batch_runs r
      LEFT JOIN bookkeeping_batch_revisions rev ON rev.id=(SELECT r2.id FROM bookkeeping_batch_revisions r2 WHERE r2.run_id=r.id ORDER BY r2.id DESC LIMIT 1)
      LEFT JOIN bookkeeping_batch_apply_attempts_v2 a ON a.id=(SELECT a2.id FROM bookkeeping_batch_apply_attempts_v2 a2 WHERE a2.revision_id=rev.id ORDER BY a2.id DESC LIMIT 1)
      WHERE NOT (r.accounting_to < ? OR r.accounting_from > ?)
      AND (a.id IS NULL OR final_event!='completed' OR failed_events>0 OR failed_checks>0) ORDER BY r.id`,input.periodStart,cutoff); return control("BATCH_UNPOSTED_OR_FAILED",evidence.length?"blocked":"passed",true,evidence);
  }));
  items.push(protect("DOCUMENT_OUTSTANDING",()=>{
    if (!has(db,"documents","status") || !has(db,"documents","invoice_date")) return unavailable("DOCUMENT_OUTSTANDING","document status/date schema unavailable");
    const evidence=rows(db,"SELECT id,status,invoice_date FROM documents WHERE status IN ('pending','failed','needs_review') AND (invoice_date BETWEEN ? AND ? OR invoice_date IS NULL) ORDER BY id",input.periodStart,cutoff); return control("DOCUMENT_OUTSTANDING",evidence.length?"blocked":"passed",true,evidence);
  }));
  items.push(protect("PURCHASE_CASE_DOCUMENTATION",()=>{
    if (!exists(db,"current_purchase_cases") || !has(db,"purchase_case_events","documentation_outcome") || !has(db,"bank_transactions","transaction_date") || !has(db,"documents","invoice_date") || !has(db,"payables","bill_date")) return unavailable("PURCHASE_CASE_DOCUMENTATION","purchase-case documentation schema unavailable");
    const evidence: Array<Record<string, unknown>>=rows(db,`SELECT c.case_id,c.version,c.source_kind,c.source_id,c.source_fingerprint,c.documentation_outcome,c.actor,c.program,
      CASE c.source_kind WHEN 'bank_transaction' THEN bt.transaction_date WHEN 'document' THEN d.invoice_date WHEN 'payable' THEN p.bill_date END AS source_date
      FROM current_purchase_cases c
      LEFT JOIN bank_transactions bt ON c.source_kind='bank_transaction' AND bt.id=c.source_id
      LEFT JOIN documents d ON c.source_kind='document' AND d.id=c.source_id
      LEFT JOIN payables p ON c.source_kind='payable' AND p.id=c.source_id
      WHERE (CASE c.source_kind WHEN 'bank_transaction' THEN bt.transaction_date WHEN 'document' THEN d.invoice_date WHEN 'payable' THEN p.bill_date END BETWEEN ? AND ?)
        OR (CASE c.source_kind WHEN 'bank_transaction' THEN bt.transaction_date WHEN 'document' THEN d.invoice_date WHEN 'payable' THEN p.bill_date END IS NULL)
      ORDER BY c.case_id`,input.periodStart,cutoff).map(row=>{const source={kind:row.source_kind,id:Number(row.source_id)} as PurchaseCaseSource;const currentFingerprint=purchaseCaseSourceFingerprint(db,source);const stale=currentFingerprint!==row.source_fingerprint;return {...row,sourceStatus:stale?"stale":"current",currentSourceFingerprint:currentFingerprint,resolutionKey:`purchase-case:${row.case_id}:v${row.version}:${row.source_fingerprint}`,vatEligibility:"separate_vat_preflight_required",need:stale?"The source changed; reassess it against the current evidence before closing the period.":undefined};});
    const unresolved=evidence.filter(row=>row.documentation_outcome==="unresolved");
    const alternative=evidence.filter(row=>row.documentation_outcome==="alternative_evidence_assessed");
    // Alternative evidence is an auditable documentation outcome only. It is
    // intentionally not a VAT approval; VAT_PREFLIGHT remains authoritative.
    const stale=evidence.filter(row=>row.sourceStatus==="stale");
    return control("PURCHASE_CASE_DOCUMENTATION",stale.length||unresolved.length?"blocked":alternative.length?"warning":"passed",false,evidence);
  }));
  items.push(protect("PAYABLE_OUTSTANDING",()=>{
    if (!has(db,"payables","due_date") || !has(db,"payables","gross_amount") || !has(db,"payable_payments","payable_id") || !has(db,"payable_payments","amount")) return unavailable("PAYABLE_OUTSTANDING","payable/payment schema unavailable");
    const evidence=rows(db,`SELECT p.id,p.due_date,p.gross_amount,COALESCE(SUM(pp.amount),0) AS paid_amount FROM payables p LEFT JOIN payable_payments pp ON pp.payable_id=p.id AND pp.payment_date<=? WHERE p.due_date<=? GROUP BY p.id HAVING COALESCE(SUM(pp.amount),0)<p.gross_amount ORDER BY p.id`,cutoff,cutoff); return control("PAYABLE_OUTSTANDING",evidence.length?"warning":"passed",true,evidence,evidence.reduce((n,r)=>n+Math.abs(Number(r.gross_amount??0)-Number(r.paid_amount??0)),0));
  }));
  items.push(protect("RECEIVABLE_OUTSTANDING",()=>{
    // Issued invoices and their append-only payment applications are the
    // canonical receivables model. Never infer a debtor balance from an
    // uploaded purchase document.
    if (!has(db,"documents","document_type") || !has(db,"documents","invoice_date") || !has(db,"documents","amount_inc_vat") || !has(db,"invoice_payments","invoice_document_id") || !has(db,"invoice_payments","payment_date") || !has(db,"invoice_payments","amount")) return unavailable("RECEIVABLE_OUTSTANDING","canonical receivable schema unavailable");
    const evidence=rows(db,`SELECT d.id,d.invoice_date,d.amount_inc_vat,
      COALESCE(SUM(CASE WHEN p.payment_date<=? THEN p.amount ELSE 0 END),0) AS paid_amount
      FROM documents d LEFT JOIN invoice_payments p ON p.invoice_document_id=d.id
      WHERE d.document_type='issued_invoice' AND d.invoice_date<=?
      GROUP BY d.id HAVING COALESCE(SUM(CASE WHEN p.payment_date<=? THEN p.amount ELSE 0 END),0)<COALESCE(d.amount_inc_vat,0)
      ORDER BY d.id`,cutoff,cutoff,cutoff);
    return control("RECEIVABLE_OUTSTANDING",evidence.length?"warning":"passed",true,evidence,evidence.reduce((n,r)=>n+Math.abs(Number(r.amount_inc_vat??0)-Number(r.paid_amount??0)),0));
  }));
  items.push(protect("DKK_CONTROL_ACCOUNTS",()=>dkkControlAccounts(db,cutoff)));
  // A read-only snapshot has a temporary SQLite filename, so it cannot infer
  // the original company directory for document-evidence verification. The
  // caller supplies that stable root; the exact same root is used at review
  // and close, keeping the reviewed packet hash meaningful.
  items.push(protect("LEDGER_AUDIT_CHAIN",()=>{const r=verifyAuditChain(db,{companyRoot:input.companyRoot});return control("LEDGER_AUDIT_CHAIN",r.ok?"passed":"blocked",false,r.errors.map(error=>({error})));}));
  items.push(protect("AUDIT_LOG_INTEGRITY",()=>{const r=verifyAuditLogIntegrity(db,{journalCrossCheck:false});return control("AUDIT_LOG_INTEGRITY",r.ok?"passed":"blocked",false,r.errors.map(error=>({error})));}));
  items.push(protect("TRIAL_BALANCE",()=>{const r=buildTrialBalance(db,input.periodStart,cutoff);return control("TRIAL_BALANCE",r.ok&&r.balanced?"passed":"blocked",false,r.ok&&r.balanced?[]:[{ok:r.ok,balanced:r.balanced}]);}));
  items.push(protect("VAT_PREFLIGHT",()=>{const r=buildVatReport(db,input.periodStart,cutoff);return control("VAT_PREFLIGHT",r.ok?"passed":"blocked",false,[...r.errors.map(error=>({error})),{filingReceipt:"not-modelled; this is a calculation/preflight, not evidence of submission"}]);}));
  items.push(protect("SQLITE_INTEGRITY",()=>{const result=db.query("PRAGMA integrity_check").all() as Array<{integrity_check?:string}>;const ok=result.length===1&&result[0]?.integrity_check==="ok";return control("SQLITE_INTEGRITY",ok?"passed":"blocked",false,ok?[]:result.map(row=>({result:row.integrity_check??null})));}));
  const sorted=items.sort((a,b)=>a.code.localeCompare(b.code));
  const body={version:4 as const,periodStart:input.periodStart,periodEnd:input.periodEnd,cutoff,controlsRun:sorted.map(item=>item.code),items:sorted,blockers:sorted.filter(item=>item.status==="blocked"||item.status==="unavailable").length,warnings:sorted.filter(item=>item.status==="warning").length};
  return {...body,hash:closeReadinessDigest(body)};
}
export const createPeriodCloseReadinessPacket=computePeriodCloseReadiness;
function assertReviewSchema(db:Database):void { if(!periodCloseReviewSchemaAvailable(db)) throw new Error("period close review schema migration is required; run migrate first"); }
function principalJson(principal:CloseReviewPrincipal|undefined|null):string|null { if(!principal)return null;if(!principal.subjectId.trim())throw new Error("reviewer principal subject is required");return canonicalCloseReadiness(principal); }
function parsePrincipal(raw:string|null):CloseReviewPrincipal|null { if(!raw)return null;const v=JSON.parse(raw) as CloseReviewPrincipal;if(!["user","service-account","local-trusted"].includes(v.kind)||!v.subjectId)return null;return v; }
export function reviewPeriodCloseReadiness(db:Database,input:{packet:CloseReadinessPacket;reviewerActor:string;reviewerPrincipal?:CloseReviewPrincipal|null}):PeriodCloseReview { assertReviewSchema(db);const actor=input.reviewerActor.trim();if(!actor)throw new Error("reviewer actor is required");db.query("INSERT OR IGNORE INTO period_close_readiness_packets(packet_hash,period_start,period_end,cutoff,packet_json) VALUES(?,?,?,?,?)").run(input.packet.hash,input.packet.periodStart,input.packet.periodEnd,input.packet.cutoff,canonicalCloseReadiness(input.packet));const p=db.query("SELECT id FROM period_close_readiness_packets WHERE packet_hash=?").get(input.packet.hash) as {id:number};const row=db.query("INSERT INTO period_close_reviews(packet_id,packet_hash,reviewer_actor,reviewer_principal) VALUES(?,?,?,?) RETURNING id,created_at").get(p.id,input.packet.hash,actor,principalJson(input.reviewerPrincipal)) as {id:number;created_at:string};return{id:row.id,packet:input.packet,reviewerActor:actor,reviewerPrincipal:input.reviewerPrincipal??null,createdAt:row.created_at}; }
export function loadPeriodCloseReview(db:Database,id:number):PeriodCloseReview|null { if(!exists(db,"period_close_reviews"))return null;const row=db.query("SELECT r.id,r.reviewer_actor,r.reviewer_principal,r.created_at,p.packet_json FROM period_close_reviews r JOIN period_close_readiness_packets p ON p.id=r.packet_id WHERE r.id=?").get(id) as {id:number;reviewer_actor:string;reviewer_principal:string|null;created_at:string;packet_json:string}|null;return row?{id:row.id,packet:JSON.parse(row.packet_json) as CloseReadinessPacket,reviewerActor:row.reviewer_actor,reviewerPrincipal:parsePrincipal(row.reviewer_principal),createdAt:row.created_at}:null; }
export function recordPeriodCloseDecision(db:Database,input:{periodId:number;packet:CloseReadinessPacket;decision:"closed"|"forced_closed"|"reopened";actor:string;reason?:string;supersedesDecisionId?:number}):number{return(db.query("INSERT INTO period_close_decisions(period_id,packet_hash,decision,actor,reason,supersedes_decision_id) VALUES(?,?,?,?,?,?) RETURNING id").get(input.periodId,input.packet.hash,input.decision,input.actor,input.reason?.trim()||null,input.supersedesDecisionId??null)as{id:number}).id;}
export function recordForcedPeriodCloseOpenItems(db:Database,periodId:number,decisionId:number,packet:CloseReadinessPacket,reason:string,actor:string):void{for(const open of packet.items.filter(item=>item.waivable&&item.status==="blocked"))db.query("INSERT INTO period_close_open_items(decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor) VALUES(?,?,?,?,?,?,?,?,?,?)").run(decisionId,periodId,packet.hash,open.code,"blocker",open.count,open.amount,canonicalCloseReadiness(open.evidence),reason,actor);}
export function latestPeriodCloseDecision(db:Database,periodId:number):number|undefined{return(db.query("SELECT id FROM period_close_decisions WHERE period_id=? ORDER BY id DESC LIMIT 1").get(periodId)as{id:number}|null)?.id;}
export function listPeriodCloseOpenItems(db:Database,periodId:number){return db.query("SELECT id,decision_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor,created_at FROM period_close_open_items WHERE period_id=? ORDER BY id").all(periodId);}
