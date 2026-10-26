import { canonicalJson } from "./canonical-json";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { suggestBankMatches } from "./bank-suggest-matches";
import { applyPostingRuleEvaluationInCurrentTransaction, evaluatePostingRules, type PostingRuleContext } from "./posting-rules";
import { applyStoredPurchaseVatPreflightInCurrentTransaction, inspectPurchaseVatPreflight } from "./purchase-vat-preflight";
import { bookExpenseFromBankInCurrentTransaction } from "./expense-booking";
import { applyDimensionAssignment, planDimensionAssignment } from "./accounting-dimensions";
import { verifyAuditChain } from "./ledger";
import { buildTrialBalance } from "./financial-statements";
import { buildBankReconciliationReport } from "./reconciliation";
import { buildVatReport } from "./vat";
import { evaluateAccountingApproval, getAccountingApprovalPolicy } from "./accounting-approval-policy";

export type BookkeepingBatchPartition = "ready" | "suggestedMatch" | "missingDocument" | "humanDecision";
export type BookkeepingBatchItem = { actionKey: string; evidenceHash: string; partition: BookkeepingBatchPartition; documentId?: number; bankTransactionId?: number; ruleApplication?: { ruleVersionId: number; payloadHash: string }; detail: Record<string, unknown> };
export type BookkeepingBatchPlan = { companyId: number; accountingFrom: string; accountingTo: string; bankFrom: string; bankTo: string; items: BookkeepingBatchItem[]; sourceIdentities: Record<string, unknown>; candidateSetHash: string; planHash: string };
export type BatchActor = { actor: string };
/** Authorization identity, deliberately independent from the audit actor. */
export type BatchPrincipal = { kind: "user" | "service-account" | "local-trusted"; subjectId: string };
export type BatchApprovalContext = { controlDb: Database; workspaceRoot: string; companySlug: string; expectedPolicyEventHash?: string | null };
type BatchIdentity = BatchActor & { principal: BatchPrincipal };
export type FinalCheckName = "audit_chain" | "trial_balance" | "reconciliation" | "vat";
export type FinalCheckDetail = Record<string, unknown> | { ok: boolean };

const date = (v: string) => /^\d{4}-\d{2}-\d{2}$/.test(v);

function hash(value: unknown) { return createHash("sha256").update(canonicalJson(value)).digest("hex"); }
function validActor(actor: string | undefined): actor is string { return typeof actor === "string" && /^(user|agent|system):[^\s]+$/.test(actor); }
function batchPrincipal(db: Database, input: { principal?: BatchPrincipal }): BatchPrincipal {
  const supplied = input.principal;
  if (supplied && ["user", "service-account", "local-trusted"].includes(supplied.kind) && supplied.subjectId.trim()) return supplied;
  // CLI/direct-core compatibility: this is a stable ledger-local subject, not
  // caller supplied actor text. Hosted adapters always provide an authenticated
  // user/service-account principal.
  return { kind: "local-trusted", subjectId: `ledger-company:${selectedCompanyId(db)}` };
}
function selectedCompanyId(db: Database): number { const rows = db.query("SELECT id FROM companies ORDER BY id").all() as Array<{ id: number }>; if (rows.length !== 1) throw new Error("selected ledger must contain exactly one company"); return rows[0]!.id; }

/**
 * Durable batch state is a projection of immutable revision, approval and
 * attempt records.  There is deliberately no mutable "run status" column.
 */
export function getBookkeepingBatchState(db: Database, runId: number) {
  const run = db.query("SELECT id AS runId,run_key AS runKey,plan_hash AS planHash,plan_json AS plan,created_at AS createdAt FROM bookkeeping_batch_runs WHERE id=?").get(runId);
  if (!run) return null;
  const revisions = db.query(`SELECT r.id AS revisionId,r.revision_hash AS planHash,r.candidate_set_hash AS candidateSetHash,
    r.planner_kind AS plannerKind,r.planner_subject_id AS plannerSubjectId,r.planner_actor AS plannerActor,r.created_at AS createdAt,
    a.principal_kind AS approverKind,a.principal_subject_id AS approverSubjectId,a.actor AS approverActor,a.approval_policy_hash AS approvalPolicyHash,a.created_at AS approvedAt
    FROM bookkeeping_batch_revisions r LEFT JOIN bookkeeping_batch_revision_approvals a ON a.revision_id=r.id
    WHERE r.run_id=? ORDER BY r.id`).all(runId);
  const attempts = db.query(`SELECT a.id AS attemptId,a.plan_hash AS planHash,a.principal_kind AS principalKind,a.principal_subject_id AS principalSubjectId,
    a.actor,a.started_at AS startedAt,e.event_type AS eventType,e.action_key AS actionKey,e.detail_json AS detail,e.created_at AS createdAt
    FROM bookkeeping_batch_apply_attempts_v2 a LEFT JOIN bookkeeping_batch_apply_events_v2 e ON e.apply_attempt_id=a.id
    WHERE a.revision_id IN (SELECT id FROM bookkeeping_batch_revisions WHERE run_id=?) ORDER BY a.id,e.id`).all(runId);
  const receipts = db.query("SELECT action_key AS actionKey,receipt_json AS receipt,created_at AS createdAt FROM bookkeeping_batch_item_receipts WHERE run_id=? ORDER BY id").all(runId);
  const finalChecks = db.query(`SELECT c.apply_attempt_id AS attemptId,c.check_name AS name,c.ok,c.detail_json AS detail,c.created_at AS createdAt
    FROM bookkeeping_batch_final_checks_v2 c WHERE c.apply_attempt_id IN
    (SELECT id FROM bookkeeping_batch_apply_attempts_v2 WHERE revision_id IN (SELECT id FROM bookkeeping_batch_revisions WHERE run_id=?)) ORDER BY c.id`).all(runId);
  return { run, revisions, attempts, receipts, finalChecks };
}
function scope(db: Database, input: { companyId?: number; accountingFrom: string; accountingTo: string; bankFrom: string; bankTo: string }) { const companyId = selectedCompanyId(db); if (input.companyId !== undefined && input.companyId !== companyId) throw new Error("company identity is derived from the selected ledger"); const result = { companyId, accountingFrom: input.accountingFrom, accountingTo: input.accountingTo, bankFrom: input.bankFrom, bankTo: input.bankTo }; if (![result.accountingFrom, result.accountingTo, result.bankFrom, result.bankTo].every(date) || result.accountingFrom > result.accountingTo || result.bankFrom > result.bankTo) throw new Error("ordered explicit ISO accounting and bank date ranges are required"); return result; }

type PurchaseEvidence = { document: { id: number; invoice_date: string | null; sender_name: string | null; sender_vat_cvr: string | null; supplier_country_code: string | null; document_type: string; currency: string; amount_inc_vat: number | null; vat_amount: number | null; sha256_hash: string | null }; bank: { id: number; transaction_date: string; amount: number; currency: string; transaction_hash: string | null; text: string }; context: PostingRuleContext };
/** Canonical loader shared by planning, stale detection, and real application. */
function loadPurchaseEvidence(db: Database, companyId: number, documentId: number, bankTransactionId: number): PurchaseEvidence | null {
  const document = db.query("SELECT id,invoice_date,sender_name,sender_vat_cvr,supplier_country_code,document_type,currency,amount_inc_vat,vat_amount,sha256_hash FROM documents WHERE id=?").get(documentId) as PurchaseEvidence["document"] | null;
  const bank = db.query("SELECT id,transaction_date,amount,currency,transaction_hash,text FROM bank_transactions WHERE id=?").get(bankTransactionId) as PurchaseEvidence["bank"] | null;
  if (!document || !bank) return null;
  return { document, bank, context: { company: companyId, documentId, supplierIdentity: document.sender_name ?? undefined, supplierCountry: document.supplier_country_code ?? undefined, supplierVat: document.sender_vat_cvr ?? undefined, documentType: document.document_type, currency: document.currency, amount: Number(document.amount_inc_vat ?? 0), vatAmount: Number(document.vat_amount ?? 0) } };
}
function evidenceHash(evidence: PurchaseEvidence) { return hash({ document: evidence.document, bank: evidence.bank }); }
/** Party defaults are review hints only: a posting rule and VAT preflight stay
 * the exclusive gates for a batch becoming executable. The snapshot was bound
 * to the document link, so this read never reaches across into workspace DB. */
function partyDefaultSuggestion(db: Database, documentId: number) {
  const row=db.query("SELECT party_id,party_role,party_snapshot_json FROM current_document_party_links WHERE document_id=? ORDER BY CASE party_role WHEN 'supplier' THEN 0 WHEN 'vendor' THEN 1 ELSE 2 END,id DESC LIMIT 1").get(documentId) as {party_id:string;party_role:string;party_snapshot_json:string}|null;
  if(!row)return null;
  try { const snapshot=JSON.parse(row.party_snapshot_json) as {roles?:Array<{role?:string;defaults?:unknown}>}; const role=snapshot.roles?.find(item=>item.role===row.party_role); const defaults=role?.defaults; if(!defaults||typeof defaults!=="object"||Array.isArray(defaults))return null; const d=defaults as Record<string,unknown>; return {partyId:row.party_id,role:row.party_role,account:typeof d.account==="string"?d.account:null,vat:typeof d.vat==="string"?d.vat:null,currency:typeof d.currency==="string"?d.currency:null}; } catch { return null; }
}
function sourceIdentities(db: Database, plan: Omit<BookkeepingBatchPlan, "sourceIdentities" | "candidateSetHash" | "planHash">, ignoreRunId?: number) {
  const ready = plan.items.filter((item) => item.partition === "ready").map((item) => {
    // A receipt is the immutable effect of this exact reviewed revision. Its
    // own rule/VAT/journal evidence must not make a resumable run stale.
    const prior = ignoreRunId && db.query("SELECT 1 FROM bookkeeping_batch_item_receipts WHERE run_id=? AND action_key=?").get(ignoreRunId, item.actionKey);
    const planned = ((plan as BookkeepingBatchPlan).sourceIdentities as any)?.ready?.find((row: any) => row.actionKey === item.actionKey);
    if (prior && planned) return planned;
    const evidence = item.documentId && item.bankTransactionId ? loadPurchaseEvidence(db, plan.companyId, item.documentId, item.bankTransactionId) : null;
    const rule = evidence ? evaluatePostingRules(db, evidence.context, { at: evidence.document.invoice_date ?? plan.accountingTo }) : null;
    const vat = item.documentId ? inspectPurchaseVatPreflight(db, item.documentId) : null;
    const vatEvent = item.documentId ? db.query(`SELECT id,event_type,supplier_country_code,supplier_identifier,classification,provider_status,evidence_expires_at,detail_json,created_at
      FROM vat_validation_events WHERE document_id=? AND event_type='preflight_passed' ORDER BY id DESC LIMIT 1`).get(item.documentId) : null;
    const reconciliation = item.bankTransactionId ? db.query("SELECT bank_transaction_id FROM bank_journal_reconciliations WHERE bank_transaction_id=? LIMIT 1").get(item.bankTransactionId) : null;
    const requestedDimensions=rule?.decision==="proposed" ? Object.entries(rule.outcome.dimensions??{}).sort(([a],[b])=>a.localeCompare(b)).map(([dimensionId,memberId])=>({dimensionId,memberId,current:db.query("SELECT status FROM current_accounting_dimension_members WHERE dimension_id=? AND member_id=?").get(dimensionId,memberId)??null})) : [];
    return { actionKey: item.actionKey, evidenceHash: evidence ? evidenceHash(evidence) : null, reconciliation: reconciliation ? "reconciled" : "unreconciled", rule: rule?.decision === "proposed" ? { ruleVersionId: rule.ruleVersionId, payloadHash: rule.payloadHash, dimensions:requestedDimensions } : { decision: rule?.decision ?? "missing" }, vat: vat ? { ok: vat.ok, classification: vat.classification, evidenceExpiresAt: vat.evidenceExpiresAt, event: vatEvent ? { id: (vatEvent as any).id, hash: hash(vatEvent) } : null } : null };
  });
  // Candidate universe is intentionally broader than executable items.  A new
  // eligible document/bank row must stale a reviewed plan instead of silently
  // being omitted on apply. Reconciliations and in-scope journal links are
  // included because they change both eligibility and the legal evidence.
  const banks = db.query(`SELECT bt.id,bt.transaction_date,bt.amount,bt.currency,bt.transaction_hash,bt.text,
    CASE WHEN EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=bt.id)
      AND NOT EXISTS(SELECT 1 FROM bookkeeping_batch_applied_links l WHERE l.run_id=? AND l.bank_transaction_id=bt.id)
      THEN 1 ELSE 0 END AS reconciled
    FROM bank_transactions bt WHERE bt.transaction_date BETWEEN ? AND ? ORDER BY bt.id`).all(ignoreRunId ?? -1, plan.bankFrom, plan.bankTo);
  // Mirror `openPurchaseDocuments` exactly. It deliberately has no invoice
  // date filter: an undated or older still-open DKK purchase can become the
  // best suggestion for an in-scope bank line.
  const documents = db.query(`SELECT d.id,d.sha256_hash,d.invoice_date,d.document_type,d.currency,d.amount_inc_vat,d.vat_amount,
    d.sender_name,d.sender_vat_cvr,d.supplier_country_code,
    (SELECT e.enriched_metadata_sha256 FROM document_metadata_enrichments e WHERE e.document_id=d.id) AS enrichments,
    (SELECT c.context_sha256 FROM document_company_contexts c WHERE c.document_id=d.id) AS contexts
    FROM documents d LEFT JOIN journal_entries je ON je.document_id=d.id AND je.status='posted'
    WHERE d.document_type='purchase_sale' AND d.currency='DKK'
      AND (je.id IS NULL OR EXISTS(SELECT 1 FROM bookkeeping_batch_applied_links l WHERE l.run_id=? AND l.document_id=d.id AND l.journal_entry_id=je.id)) ORDER BY d.id`).all(ignoreRunId ?? -1);
  const ledger = db.query(`SELECT id,entry_hash,previous_hash,transaction_date,source_bank_transaction_id
    FROM journal_entries WHERE transaction_date BETWEEN ? AND ? OR source_bank_transaction_id IN
      (SELECT id FROM bank_transactions WHERE transaction_date BETWEEN ? AND ?) ORDER BY id`).all(plan.accountingFrom, plan.accountingTo, plan.bankFrom, plan.bankTo)
    .filter((row: any) => !ignoreRunId || !db.query("SELECT 1 FROM bookkeeping_batch_applied_links WHERE run_id=? AND journal_entry_id=?").get(ignoreRunId,row.id));
  return { ready, candidateUniverse: { banks, documents, ledger } };
}

/** Read-only planning creates a purchase action only for one unambiguous bank/document pair. */
export type BookkeepingBatchScope = Omit<BookkeepingBatchPlan, "items" | "planHash" | "sourceIdentities" | "candidateSetHash">;
export function planBookkeepingBatch(db: Database, input: BookkeepingBatchScope): BookkeepingBatchPlan {
  const s = scope(db, input);
  const banks = db.query(`SELECT bt.id
    FROM bank_transactions bt
    WHERE bt.transaction_date BETWEEN ? AND ?
      AND NOT EXISTS (
        SELECT 1
        FROM bank_journal_reconciliations reconciliation
        WHERE reconciliation.bank_transaction_id = bt.id
      )
    ORDER BY bt.id`).all(s.bankFrom, s.bankTo) as Array<{ id: number }>;
  const pairs = banks.flatMap((bank) => { const suggested = suggestBankMatches(db, { bankTransactionId: bank.id, max: 2 }); const match = suggested.ok ? suggested.rows[0]?.suggestions[0] : undefined; return match?.documentId ? [{ bankId: bank.id, documentId: match.documentId, suggestion: match }] : []; });
  const perDocument = new Map<number, number>(); for (const pair of pairs) perDocument.set(pair.documentId, (perDocument.get(pair.documentId) ?? 0) + 1);
  const items: BookkeepingBatchItem[] = [];
  for (const pair of pairs) {
    const evidence = loadPurchaseEvidence(db, s.companyId, pair.documentId, pair.bankId); if (!evidence) continue;
    const rule = evaluatePostingRules(db, evidence.context, { at: evidence.document.invoice_date ?? s.accountingTo });
    const unambiguous = perDocument.get(pair.documentId) === 1;
    const vat = inspectPurchaseVatPreflight(db, pair.documentId);
    const ready = unambiguous && rule.decision === "proposed" && typeof rule.outcome.account === "string" && vat?.ok === true;
    const partyDefaults=partyDefaultSuggestion(db,pair.documentId);
    items.push({ actionKey: `purchase:${pair.documentId}:bank:${pair.bankId}`, evidenceHash: evidenceHash(evidence), partition: ready ? "ready" : "humanDecision", documentId: pair.documentId, bankTransactionId: pair.bankId, ruleApplication: rule.decision === "proposed" ? { ruleVersionId: rule.ruleVersionId, payloadHash: rule.payloadHash } : undefined, detail: { suggestion: pair.suggestion, unambiguous, vatReady: vat?.ok === true, partyDefaults: partyDefaults ? { ...partyDefaults, advisoryOnly:true, neverOverrides:{postingRule:true,vatEvidence:true} } : null, rule: rule.decision === "proposed" ? { ruleVersionId: rule.ruleVersionId, payloadHash: rule.payloadHash, outcome: rule.outcome } : { reasons: rule.reasons } } });
  }
  for (const bank of banks) if (!pairs.some(pair => pair.bankId === bank.id)) items.push({ actionKey: `bank:${bank.id}`, evidenceHash: hash({ bankId: bank.id }), partition: "missingDocument", bankTransactionId: bank.id, detail: {} });
  items.sort((a, b) => a.actionKey.localeCompare(b.actionKey));
  const draft = { ...s, items };
  const identities = sourceIdentities(db, draft);
  const candidateSetHash = hash(identities.candidateUniverse);
  return { ...draft, sourceIdentities: identities, candidateSetHash, planHash: hash({ ...draft, sourceIdentities: identities, candidateSetHash }) };
}

function event(db: Database, runId: number, type: "planned" | "approved" | "apply_started" | "final_checks" | "completed", planHash: string, actor: string, detail: unknown = {}) { db.query("INSERT INTO bookkeeping_batch_events(run_id,event_type,plan_hash,actor,detail_json,created_at) VALUES(?,?,?,?,?,?)").run(runId, type, planHash, actor, canonicalJson(detail), new Date().toISOString()); }
export function createBookkeepingBatchRun(db: Database, input: BookkeepingBatchPlan & BatchActor & { principal?: BatchPrincipal; runKey?: string }) {
  if (!validActor(input.actor) || !input.runKey?.trim()) throw new Error("actor and runKey are required");
  const runKey = input.runKey;
  const plan = { companyId: input.companyId, accountingFrom: input.accountingFrom, accountingTo: input.accountingTo, bankFrom: input.bankFrom, bankTo: input.bankTo, items: input.items, sourceIdentities: input.sourceIdentities, candidateSetHash: input.candidateSetHash, planHash: input.planHash };
  const stored = canonicalJson(plan); if (hash({ companyId:plan.companyId, accountingFrom:plan.accountingFrom, accountingTo:plan.accountingTo, bankFrom:plan.bankFrom, bankTo:plan.bankTo, items:plan.items, sourceIdentities:plan.sourceIdentities, candidateSetHash:plan.candidateSetHash }) !== input.planHash) throw new Error("invalid canonical plan JSON or planHash");
  const principal = batchPrincipal(db, input);
  return db.transaction(() => { const old = db.query("SELECT id,plan_hash,plan_json FROM bookkeeping_batch_runs WHERE run_key=?").get(runKey) as any; if (old) { if (old.plan_hash !== input.planHash) throw new Error("runKey already binds another plan"); return { runId: old.id, duplicate: true, plan: JSON.parse(old.plan_json) as BookkeepingBatchPlan }; } const row = db.query("INSERT INTO bookkeeping_batch_runs(run_key,company_id,accounting_from,accounting_to,bank_from,bank_to,plan_hash,plan_json,created_at) VALUES(?,?,?,?,?,?,?,?,?) RETURNING id").get(runKey, input.companyId, input.accountingFrom, input.accountingTo, input.bankFrom, input.bankTo, input.planHash, stored, new Date().toISOString()) as { id: number }; db.query("INSERT INTO bookkeeping_batch_revisions(run_id,revision_hash,candidate_set_hash,plan_json,planner_kind,planner_subject_id,planner_actor,created_at) VALUES(?,?,?,?,?,?,?,?)").run(row.id,input.planHash,input.candidateSetHash,stored,principal.kind,principal.subjectId,input.actor,new Date().toISOString()); event(db, row.id, "planned", input.planHash, input.actor, { candidateSetHash: input.candidateSetHash }); return { runId: row.id, duplicate: false, plan }; }).immediate();
}
export function approveBookkeepingBatchPlan(db: Database, input: BatchActor & { principal?: BatchPrincipal; runId: number; planHash: string; approval?: BatchApprovalContext }) { if (!validActor(input.actor)) throw new Error("actor is required"); const principal=batchPrincipal(db,input); return db.transaction(() => { const revision = db.query("SELECT id,planner_kind,planner_subject_id FROM bookkeeping_batch_revisions WHERE run_id=? AND revision_hash=?").get(input.runId,input.planHash) as {id:number;planner_kind:BatchPrincipal['kind'];planner_subject_id:string}|null; if (!revision) throw new Error("exact pending plan was not found"); let policyHash:string|null=null; if(input.approval){const active=getAccountingApprovalPolicy(input.approval.controlDb,input.approval.companySlug);if(active&&input.approval.expectedPolicyEventHash!==active.eventHash)throw new Error("STALE_APPROVAL_POLICY");const decision=evaluateAccountingApproval(input.approval.controlDb,input.approval.workspaceRoot,{companySlug:input.approval.companySlug,action:"bookkeeping_batch_approve",principalId:principal.subjectId,proposedByPrincipalId:revision.planner_subject_id});if(!decision.allowed)throw new Error(decision.code);policyHash=decision.policy?.eventHash??null;} else if(revision.planner_kind===principal.kind && revision.planner_subject_id===principal.subjectId) throw new Error("SELF_APPROVAL_FORBIDDEN"); const existing=db.query("SELECT 1 FROM bookkeeping_batch_revision_approvals WHERE revision_id=?").get(revision.id); if (!existing) { db.query("INSERT INTO bookkeeping_batch_revision_approvals(revision_id,principal_kind,principal_subject_id,actor,approval_policy_hash,created_at) VALUES(?,?,?,?,?,?)").run(revision.id,principal.kind,principal.subjectId,input.actor,policyHash,new Date().toISOString()); event(db,input.runId,"approved",input.planHash,input.actor,{principal:{kind:principal.kind,subjectId:principal.subjectId},approvalPolicyHash:policyHash}); } return { ok: true as const }; }).immediate(); }

function applyPurchaseAction(db: Database, plan: BookkeepingBatchPlan, item: BookkeepingBatchItem, actor: string, principal: BatchPrincipal) {
  if (!item.documentId || !item.bankTransactionId) throw new Error("purchase action lacks exact document and bank evidence");
  const evidence = loadPurchaseEvidence(db, plan.companyId, item.documentId, item.bankTransactionId); if (!evidence || evidenceHash(evidence) !== item.evidenceHash) return { outcome: "stale" as const, error: "evidence changed" };
  const rule = applyPostingRuleEvaluationInCurrentTransaction(db, evidence.context, { applicationKey: `${item.actionKey}:rule`, at: evidence.document.invoice_date ?? plan.accountingTo });
  if (rule.decision !== "proposed" || !rule.applicationId || typeof rule.outcome.account !== "string") throw new Error(rule.reasons.join("; ") || "posting rule blocked purchase application");
  const vat = applyStoredPurchaseVatPreflightInCurrentTransaction(db, item.documentId, { actor });
  if (!vat.ok || !vat.vatPreflightId) throw new Error(vat.errors.join("; ") || "VAT preflight blocked purchase application");
  const posted = bookExpenseFromBankInCurrentTransaction(db, { documentId: item.documentId, bankTransactionId: item.bankTransactionId, expenseAccountNo: rule.outcome.account, vatTreatment: rule.outcome.vatTreatment as any, createdBy: actor, createdByProgram: "bookkeeping-batch" });
  if (!posted.ok || !posted.entryId) throw new Error(posted.errors.join("; ") || "purchase posting failed");
  const requestedDimensions=Object.entries(rule.outcome.dimensions??{});
  const dimensionAssignmentIds:number[]=[];
  if(requestedDimensions.length){
    const line=db.query(`SELECT jl.id,jl.debit_amount,jl.credit_amount,jl.currency FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=? AND a.account_no=? ORDER BY jl.id LIMIT 1`).get(posted.entryId,rule.outcome.account) as any;
    if(!line) throw new Error("dimension assignment lacks resulting expense line");
    const amountMinor=Math.round(Math.abs((Number(line.debit_amount)-Number(line.credit_amount))*100));
    const allocations=requestedDimensions.map(([dimensionId,memberId])=>({dimensionId,memberId:String(memberId),amountMinor,currency:String(line.currency)}));
    const proposed=planDimensionAssignment(db,{journalLineId:line.id,allocations,source:"reviewed"});
    if(!proposed.ok) throw new Error(`dimension assignment blocked: ${proposed.errors.join(",")}`);
    const assignment=applyDimensionAssignment(db,{journalLineId:line.id,allocations,source:"reviewed",planHash:proposed.plan.planHash,confirm:true,actor,principal:`${principal.kind}:${principal.subjectId}`,idempotencyKey:`batch:${plan.planHash}:${item.actionKey}:dimensions`});
    if(!assignment.ok) throw new Error(`dimension assignment blocked: ${assignment.errors.join(",")}`);
    dimensionAssignmentIds.push(assignment.id);
  }
  const purchase = db.query("INSERT INTO purchase_posting_applications(document_id,application_key,journal_entry_id,bank_transaction_id,status) VALUES(?,?,?,?,?) RETURNING id").get(item.documentId, `${item.actionKey}:purchase`, posted.entryId, item.bankTransactionId, "posted") as { id: number };
  return { outcome: "applied" as const, documentId: item.documentId, bankTransactionId: item.bankTransactionId, journalEntryId: Number(posted.entryId), vatPreflightId: vat.vatPreflightId, postingRuleApplicationId: rule.applicationId, purchaseApplicationId: purchase.id, dimensionAssignmentIds };
}

export function applyBookkeepingBatch(db: Database, input: BatchActor & { principal?: BatchPrincipal; runId: number; planHash: string; finalChecks?: Partial<Record<FinalCheckName, () => { ok: boolean; detail?: FinalCheckDetail }>>; /** Test-only fault seam; no transport adapter supplies this. */ testOnly?: { beforeItem?: (actionKey: string) => void; afterItem?: (actionKey: string) => void } }) {
  if (!validActor(input.actor)) throw new Error("actor is required for apply");
  const principal=batchPrincipal(db,input);
  // The entire verification, application and evidence write is one IMMEDIATE
  // transaction.  A second connection cannot insert a newly eligible source
  // between the freshness check and the first posting.
  return db.transaction(() => {
  const revision=db.query("SELECT r.id,r.plan_json,r.candidate_set_hash FROM bookkeeping_batch_revisions r WHERE r.run_id=? AND r.revision_hash=?").get(input.runId,input.planHash) as {id:number;plan_json:string;candidate_set_hash:string}|null;
  if (!revision || !db.query("SELECT 1 FROM bookkeeping_batch_revision_approvals WHERE revision_id=?").get(revision.id)) throw new Error("approved planHash is required");
  const plan=JSON.parse(revision.plan_json) as BookkeepingBatchPlan;
  // Earlier receipts are this exact revision's immutable effects, not external
  // source drift. Normalise only those effects before comparing sources.
  const actualSources=sourceIdentities(db,plan,input.runId);
  const attempt=db.query("INSERT INTO bookkeeping_batch_apply_attempts_v2(revision_id,plan_hash,principal_kind,principal_subject_id,actor,started_at) VALUES(?,?,?,?,?,?) RETURNING id").get(revision.id,input.planHash,principal.kind,principal.subjectId,input.actor,new Date().toISOString()) as {id:number};
  const applyEvent=(type:"started"|"source_stale"|"item_applied"|"item_failed"|"final_checks"|"completed",detail:unknown,actionKey?:string)=>db.query("INSERT INTO bookkeeping_batch_apply_events_v2(apply_attempt_id,event_type,action_key,detail_json,created_at) VALUES(?,?,?,?,?)").run(attempt.id,type,actionKey??null,canonicalJson(detail),new Date().toISOString());
  applyEvent("started",{candidateSetHash:plan.candidateSetHash});
  if (canonicalJson(actualSources)!==canonicalJson(plan.sourceIdentities) || hash((actualSources as any).candidateUniverse)!==revision.candidate_set_hash) {
    applyEvent("source_stale",{code:"STALE_PLAN", expectedPlanHash:input.planHash, expectedCandidateSetHash:revision.candidate_set_hash, actualCandidateSetHash:hash((actualSources as any).candidateUniverse)});
    event(db,input.runId,"apply_started",input.planHash,input.actor,{stale:true,cause:"SOURCE_EVIDENCE_CHANGED"});
    return { ok:false,errors:["STALE_PLAN"],error:{code:"STALE_PLAN",cause:"SOURCE_EVIDENCE_CHANGED"},results:[],checks:[] };
  }
  event(db,input.runId,"apply_started",input.planHash,input.actor,{applyAttemptId:attempt.id});
  const results: Array<{ actionKey: string; outcome: string; error?: string }> = [];
  for (const [index,item] of plan.items.filter(x => x.partition === "ready").entries()) {
    if (db.query("SELECT 1 FROM bookkeeping_batch_item_receipts WHERE run_id=? AND action_key=?").get(input.runId,item.actionKey)) { results.push({actionKey:item.actionKey,outcome:"duplicate"}); continue; }
    const savepoint=`batch_item_${index}`;
    db.exec(`SAVEPOINT ${savepoint}`);
    try { input.testOnly?.beforeItem?.(item.actionKey); db.query("INSERT INTO bookkeeping_batch_item_attempts(run_id,action_key,evidence_hash,outcome,created_at) VALUES(?,?,?,?,?)").run(input.runId,item.actionKey,item.evidenceHash,"started",new Date().toISOString()); const applied=applyPurchaseAction(db,plan,item,input.actor,principal); if(applied.outcome!=="applied") throw new Error(applied.error); if(![applied.journalEntryId,applied.vatPreflightId,applied.postingRuleApplicationId,applied.purchaseApplicationId].every(Number.isInteger)) throw new Error("purchase application did not produce immutable evidence ids"); db.query("INSERT INTO bookkeeping_batch_applied_links(run_id,action_key,document_id,journal_entry_id,bank_transaction_id,vat_preflight_id,posting_rule_application_id,purchase_application_id,evidence_hash,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").run(input.runId,item.actionKey,applied.documentId,applied.journalEntryId,applied.bankTransactionId,applied.vatPreflightId,applied.postingRuleApplicationId,applied.purchaseApplicationId,item.evidenceHash,new Date().toISOString()); db.query("INSERT INTO bookkeeping_batch_item_receipts(run_id,action_key,receipt_json,created_at) VALUES(?,?,?,?)").run(input.runId,item.actionKey,canonicalJson(applied),new Date().toISOString()); input.testOnly?.afterItem?.(item.actionKey); db.query("INSERT INTO bookkeeping_batch_item_attempts(run_id,action_key,evidence_hash,outcome,created_at) VALUES(?,?,?,?,?)").run(input.runId,item.actionKey,item.evidenceHash,"applied",new Date().toISOString()); db.exec(`RELEASE ${savepoint}`); applyEvent("item_applied",applied,item.actionKey); results.push({actionKey:item.actionKey,outcome:"applied"}); }
    catch(error) { db.exec(`ROLLBACK TO ${savepoint}`); db.exec(`RELEASE ${savepoint}`); const message=error instanceof Error?error.message:String(error); db.query("INSERT INTO bookkeeping_batch_item_attempts(run_id,action_key,evidence_hash,outcome,error_text,created_at) VALUES(?,?,?,?,?,?)").run(input.runId,item.actionKey,item.evidenceHash,"failed",message,new Date().toISOString()); applyEvent("item_failed",{error:message},item.actionKey); results.push({actionKey:item.actionKey,outcome:"failed",error:message}); }
  }
  const defaults: Record<FinalCheckName, () => { ok: boolean; detail?: FinalCheckDetail }> = { audit_chain: () => ({ ok: verifyAuditChain(db).ok }), trial_balance: () => { const x = buildTrialBalance(db, plan.accountingFrom, plan.accountingTo); return { ok: x.ok && x.balanced, detail: x }; }, reconciliation: () => { const x = buildBankReconciliationReport(db, plan.bankFrom, plan.bankTo); return { ok: x.ok && x.unmatchedCount === 0, detail: x }; }, vat: () => { const x = buildVatReport(db, plan.accountingFrom, plan.accountingTo); return { ok: x.ok, detail: x }; } };
  const checks = (Object.keys(defaults) as FinalCheckName[]).map(name => { const checked=(input.finalChecks?.[name]??defaults[name])(); db.query("INSERT INTO bookkeeping_batch_final_checks_v2(apply_attempt_id,check_name,ok,detail_json,created_at) VALUES(?,?,?,?,?)").run(attempt.id,name,checked.ok?1:0,canonicalJson(checked.detail??{}),new Date().toISOString()); return {name,...checked}; });
  applyEvent("final_checks",{checks}); event(db,input.runId,"final_checks",input.planHash,input.actor,{applyAttemptId:attempt.id,checks}); const ok=results.every(r=>r.outcome==="applied"||r.outcome==="duplicate")&&checks.every(x=>x.ok); applyEvent("completed",{ok,results}); event(db,input.runId,"completed",input.planHash,input.actor,{applyAttemptId:attempt.id,results}); return {ok,results,checks,applyAttemptId:attempt.id};
  }).immediate();
}
