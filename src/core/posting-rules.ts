import { canonicalJson } from "./canonical-json";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { recordException } from "./exceptions";
import { inspectDocumentInvoiceExtraction } from "./invoice-extraction";

export type PostingRuleState = "proposed" | "approved" | "rejected" | "disabled" | "superseded";
export type PostingRuleConditions = {
  supplierIdentity?: string;
  company: number;
  documentType?: string;
  supplierCountry?: string;
  supplierVat?: string;
  currency?: string;
  vat?: "zero" | "positive";
  amount?: { min?: number; max?: number };
  reverseChargeWording?: boolean;
  attributes?: Record<string, string>;
};
export type PostingRuleOutcome = {
  account?: string;
  vatTreatment?: string;
  textTemplate?: string;
  dimensions?: Record<string, string>;
};
export type PostingRuleContext = {
  company: number;
  documentId?: number;
  supplierIdentity?: string;
  documentType?: string;
  supplierCountry?: string;
  supplierVat?: string;
  currency?: string;
  vatAmount?: number;
  amount?: number;
  reverseChargeWording?: boolean;
  attributes?: Record<string, string>;
  /** Evidence changes always fail closed, even if a rule otherwise matches. */
  changedBuyer?: boolean;
  changedSupplier?: boolean;
  changedVat?: boolean;
  changedCurrency?: boolean;
  unusualAmount?: boolean;
  contradictoryEvidence?: boolean;
};
export type CreatePostingRuleProposalInput = {
  ruleId: string; companyId: number; effectiveFrom: string; effectiveTo?: string;
  conditions: PostingRuleConditions; outcome: PostingRuleOutcome; provenance: string;
  rationale: string; creator: string; createdAt?: string;
};

const states = new Set<PostingRuleState>(["proposed", "approved", "rejected", "disabled", "superseded"]);
const iso = (value: string) => /^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z)?$/.test(value);
const text = (value: unknown, max: number) => typeof value === "string" && value.trim().length > 0 && value.trim().length <= max;
export function postingRulePayloadHash(input: Pick<CreatePostingRuleProposalInput, "ruleId" | "companyId" | "effectiveFrom" | "effectiveTo" | "conditions" | "outcome" | "provenance" | "rationale">): string {
  return createHash("sha256").update(canonical(input)).digest("hex");
}
function validation(input: CreatePostingRuleProposalInput): string[] {
  const errors: string[] = [];
  const c = input.conditions;
  if (!text(input.ruleId, 128) || !Number.isInteger(input.companyId) || input.companyId <= 0) errors.push("rule id and positive company id are required");
  if (!iso(input.effectiveFrom) || (input.effectiveTo && (!iso(input.effectiveTo) || input.effectiveTo < input.effectiveFrom))) errors.push("effective dates must be ordered ISO dates");
  if (c.company !== input.companyId) errors.push("condition company must exactly equal rule company");
  if (c.vat && c.vat !== "zero" && c.vat !== "positive") errors.push("VAT condition must be zero or positive");
  if (c.amount && (!Number.isFinite(c.amount.min ?? 0) || !Number.isFinite(c.amount.max ?? 0) || (c.amount.min !== undefined && c.amount.max !== undefined && c.amount.min > c.amount.max))) errors.push("amount range is invalid");
  const attrs = c.attributes ?? {};
  if (Object.keys(attrs).length > 12 || Object.entries(attrs).some(([k, v]) => !/^[A-Za-z][A-Za-z0-9_.-]{0,63}$/.test(k) || !text(v, 256))) errors.push("attributes must be at most 12 bounded exact key/value pairs");
  if (Object.values(input.outcome.dimensions ?? {}).some((v) => !text(v, 256))) errors.push("dimensions must be flat bounded strings");
  if (!text(input.provenance, 256) || !text(input.rationale, 2000) || !text(input.creator, 256)) errors.push("provenance, rationale, and creator are required");
  return errors;
}
function now(input?: string) { return input ?? new Date().toISOString(); }
function versionRow(db: Database, companyId: number, ruleId: string, version: number) {
  return db.query("SELECT * FROM posting_rule_versions WHERE company_id = ? AND rule_id = ? AND version = ?").get(companyId, ruleId, version) as any | null;
}
function append(db: Database, row: any, state: PostingRuleState, actor: string, rationale: string, provenance: string, at: string, approver?: string, supersedingVersionId?: number) {
  if (!states.has(state)) throw new Error("unsupported posting rule state");
  db.query(`INSERT INTO posting_rule_lifecycle_events(rule_version_id,state,effective_at,actor,approver,rationale,provenance,expected_payload_hash,superseding_version_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)`)
    .run(row.id, state, at, actor, approver ?? null, rationale, provenance, row.payload_hash, supersedingVersionId ?? null, at);
}
export function createPostingRuleProposal(db: Database, input: CreatePostingRuleProposalInput) {
  const errors = validation(input); if (errors.length) return { ok: false as const, errors };
  const createdAt = now(input.createdAt); const payloadHash = postingRulePayloadHash(input);
  const existing = db.query("SELECT * FROM posting_rule_versions WHERE company_id=? AND rule_id=? AND payload_hash=?").get(input.companyId, input.ruleId.trim(), payloadHash) as any | null;
  if (existing) return { ok: true as const, duplicate: true, version: existing.version, payloadHash, errors: [] as string[] };
  return db.transaction(() => {
    const version = (db.query("SELECT COALESCE(MAX(version),0)+1 AS version FROM posting_rule_versions WHERE company_id=? AND rule_id=?").get(input.companyId, input.ruleId.trim()) as { version: number }).version;
    const row = db.query(`INSERT INTO posting_rule_versions(rule_id,company_id,version,effective_from,effective_to,conditions_json,outcome_json,provenance,rationale,created_by,payload_hash,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) RETURNING *`)
      .get(input.ruleId.trim(), input.companyId, version, input.effectiveFrom, input.effectiveTo ?? null, canonical(input.conditions), canonical(input.outcome), input.provenance.trim(), input.rationale.trim(), input.creator.trim(), payloadHash, createdAt) as any;
    append(db, row, "proposed", input.creator.trim(), input.rationale.trim(), input.provenance.trim(), createdAt);
    return { ok: true as const, duplicate: false, version, payloadHash, errors: [] as string[] };
  })();
}
/** A manual decision is deliberately only a proposal; it cannot self-approve. */
export const createManualPostingProposal = createPostingRuleProposal;
export type PostingRuleTransitionInput = { companyId: number; ruleId: string; version: number; state: Exclude<PostingRuleState, "proposed">; actor: string; rationale: string; provenance: string; expectedPayloadHash: string; effectiveAt?: string; supersedingVersion?: number };
type PostingRuleApprovalInput = Omit<PostingRuleTransitionInput, "state">;
export function transitionPostingRule(db: Database, input: PostingRuleTransitionInput) {
  const row = versionRow(db, input.companyId, input.ruleId, input.version);
  if (!row) return { ok: false, errors: ["posting rule version is not in this company"] };
  if (!states.has(input.state) || !text(input.actor, 256) || !text(input.rationale, 2000) || !text(input.provenance, 256)) return { ok: false, errors: ["invalid lifecycle transition"] };
  if (row.payload_hash !== input.expectedPayloadHash) return { ok: false, errors: ["explicit approval must bind the exact rule version payload hash"] };
  if (input.state === "approved" && input.actor === row.created_by) return { ok: false, errors: ["a proposal creator cannot approve their own rule"] };
  const effectiveAt = now(input.effectiveAt); if (!iso(effectiveAt)) return { ok: false, errors: ["effectiveAt must be an ISO date"] };
  const current = stateAt(db, row.id, effectiveAt);
  if ((input.state === "approved" || input.state === "rejected") && current !== "proposed") return { ok: false, errors: ["only a proposed version may be approved or rejected"] };
  if ((input.state === "disabled" || input.state === "superseded") && current !== "approved") return { ok: false, errors: ["only an approved version may be disabled or superseded"] };
  db.transaction(() => append(db, row, input.state, input.actor.trim(), input.rationale.trim(), input.provenance.trim(), effectiveAt, input.state === "approved" ? input.actor.trim() : undefined, input.supersedingVersion))();
  return { ok: true, errors: [] as string[] };
}
export const approvePostingRuleVersion = (db: Database, input: PostingRuleApprovalInput) => transitionPostingRule(db, { ...input, state: "approved" });
export const rejectPostingRuleVersion = (db: Database, input: PostingRuleApprovalInput) => transitionPostingRule(db, { ...input, state: "rejected" });
export const disablePostingRuleVersion = (db: Database, input: PostingRuleApprovalInput) => transitionPostingRule(db, { ...input, state: "disabled" });
export const supersedePostingRuleVersion = (db: Database, input: PostingRuleApprovalInput) => transitionPostingRule(db, { ...input, state: "superseded" });

function stateAt(db: Database, id: number, at: string): PostingRuleState | null {
  return (db.query("SELECT state FROM posting_rule_lifecycle_events WHERE rule_version_id=? AND effective_at<=? ORDER BY effective_at DESC,id DESC LIMIT 1").get(id, at) as { state: PostingRuleState } | null)?.state ?? null;
}
function mismatch(c: PostingRuleConditions, x: PostingRuleContext): string[] {
  const reasons: string[] = [];
  const exact: Array<[keyof PostingRuleConditions, keyof PostingRuleContext]> = [["supplierIdentity", "supplierIdentity"], ["documentType", "documentType"], ["supplierCountry", "supplierCountry"], ["supplierVat", "supplierVat"], ["currency", "currency"], ["reverseChargeWording", "reverseChargeWording"]];
  for (const [rule, context] of exact) if (c[rule] !== undefined && c[rule] !== x[context]) reasons.push(`${String(rule)} did not match exactly`);
  if (c.vat && (c.vat === "zero" ? x.vatAmount !== 0 : !(typeof x.vatAmount === "number" && x.vatAmount > 0))) reasons.push("VAT condition did not match");
  if (c.amount && (typeof x.amount !== "number" || (c.amount.min !== undefined && x.amount < c.amount.min) || (c.amount.max !== undefined && x.amount > c.amount.max))) reasons.push("amount range did not match");
  for (const [key, value] of Object.entries(c.attributes ?? {})) if (x.attributes?.[key] !== value) reasons.push(`attribute ${key} did not match exactly`);
  return reasons;
}
export function evaluatePostingRules(db: Database, context: PostingRuleContext, options: { at?: string } = {}) {
  const at = options.at ?? new Date().toISOString();
  const extracted = context.documentId ? inspectDocumentInvoiceExtraction(db, context.documentId) : null;
  // No extraction is normal for documents that were not routed through that
  // optional processor.  Only an extraction that actually exists and needs
  // resolution is contradictory evidence; absence is never a contradiction.
  const evidence = ["changedBuyer", "changedSupplier", "changedVat", "changedCurrency", "unusualAmount", "contradictoryEvidence"].filter((k) => context[k as keyof PostingRuleContext] === true || (k === "contradictoryEvidence" && extracted !== null && extracted.status !== "completed"));
  const rows = db.query("SELECT * FROM posting_rule_versions WHERE company_id=? AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY rule_id ASC,version ASC").all(context.company, at, at) as any[];
  const explanations = rows.map((row) => { const state = stateAt(db, row.id, at); const reasons = state === "approved" ? mismatch(JSON.parse(row.conditions_json), context) : [`state is ${state ?? "not yet effective"}`]; return { ruleId: row.rule_id, version: row.version, payloadHash: row.payload_hash, state, matched: reasons.length === 0, reasons }; });
  const matches = explanations.filter((x) => x.matched);
  if (evidence.length || matches.length !== 1) return { decision: "human_decision" as const, outcome: undefined, matched: matches, explanations, reasons: evidence.length ? evidence.map((x) => `${x} requires human decision`) : [matches.length === 0 ? "no approved rule matched" : "multiple approved rules matched"] };
  const match = matches[0]!; const row = rows.find((x) => x.rule_id === match.ruleId && x.version === match.version)!;
  return { decision: "proposed" as const, outcome: JSON.parse(row.outcome_json) as PostingRuleOutcome, matched: matches, explanations, reasons: [] as string[], ruleVersionId: row.id, payloadHash: row.payload_hash };
}
/**
 * Applies one immutable rule decision inside a transaction owned by the
 * caller.  This deliberately does not start a nested SQLite transaction: a
 * batch item must be able to roll its rule decision, exception and later
 * journal effects back as one unit.
 */
export function applyPostingRuleEvaluationInCurrentTransaction(db: Database, context: PostingRuleContext, input: { applicationKey: string; at?: string }) {
  const evaluation = evaluatePostingRules(db, context, { at: input.at });
  const existing = db.query("SELECT id FROM posting_rule_applications WHERE application_key=?").get(input.applicationKey) as { id: number } | null;
  if (existing) return { ...evaluation, applicationId: existing.id, duplicate: true, exceptionId: undefined };
  const createdAt = input.at ?? new Date().toISOString();
  const row = db.query("INSERT INTO posting_rule_applications(application_key,company_id,document_id,rule_version_id,rule_payload_hash,decision,explanation_json,created_at) VALUES(?,?,?,?,?,?,?,?) RETURNING id").get(input.applicationKey, context.company, context.documentId ?? null, evaluation.decision === "proposed" ? evaluation.ruleVersionId : null, evaluation.decision === "proposed" ? evaluation.payloadHash : null, evaluation.decision, canonical(evaluation), createdAt) as { id: number };
  const exception = evaluation.decision === "human_decision" ? recordException(db, { type: "POSTING_RULE_HUMAN_DECISION", severity: "medium", relatedDocumentId: context.documentId ?? null, message: evaluation.reasons.join("; "), requiredAction: "Make and explicitly approve an exact posting-rule proposal or post a reviewed journal", resolutionKey: `posting-rule:${context.company}:${context.documentId ?? input.applicationKey}`, postingPreview: evaluation }) : undefined;
  return { ...evaluation, applicationId: row.id, duplicate: false, exceptionId: exception?.exceptionId };
}
/** Standalone compatibility wrapper. */
export function applyPostingRuleEvaluation(db: Database, context: PostingRuleContext, input: { applicationKey: string; at?: string }) {
  return db.transaction(() => applyPostingRuleEvaluationInCurrentTransaction(db, context, input)).immediate();
}
/** Transaction-aware identity-link primitive for larger atomic workflows. */
export function linkDocumentVendorIdentityInCurrentTransaction(db: Database, input: { companyId: number; documentId: number; vendorId?: number; supplierIdentity: string; provenance: string; rationale: string; creator: string; createdAt?: string }) {
  if (![input.companyId, input.documentId].every((x) => Number.isInteger(x) && x > 0) || !text(input.supplierIdentity, 256) || !text(input.provenance, 256) || !text(input.rationale, 2000) || !text(input.creator, 256)) return { ok: false, errors: ["invalid company-local document/vendor identity link"] };
  try { const row = db.query("INSERT INTO document_vendor_identity_links(company_id,document_id,vendor_id,supplier_identity,provenance,rationale,created_by,created_at) VALUES(?,?,?,?,?,?,?,?) RETURNING id").get(input.companyId, input.documentId, input.vendorId ?? null, input.supplierIdentity.trim(), input.provenance.trim(), input.rationale.trim(), input.creator.trim(), now(input.createdAt)) as { id: number }; return { ok: true, id: row.id, errors: [] as string[] }; } catch { return { ok: false, errors: ["document identity is already linked in this company"] }; }
}
/** Standalone compatibility wrapper. */
export function linkDocumentVendorIdentity(db: Database, input: { companyId: number; documentId: number; vendorId?: number; supplierIdentity: string; provenance: string; rationale: string; creator: string; createdAt?: string }) {
  return db.transaction(() => linkDocumentVendorIdentityInCurrentTransaction(db, input)).immediate();
}
const canonical = canonicalJson;
