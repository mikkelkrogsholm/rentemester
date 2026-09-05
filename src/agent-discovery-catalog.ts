import { createHash } from "node:crypto";
import { RETRY_OPERATION_NAMES } from "./core/idempotency";
import { getBuildIdentity } from "./core/build-identity";
import { getReleaseProvenance } from "./core/release-provenance";
import { currentRuleBundleVersion } from "./core/rules-metadata";

export const AGENT_CATALOGUE_SCHEMA_VERSION = "rentemester-agent-discovery-v1";
export const AGENT_CATALOGUE_ENTRY_POINT = "meta_about -> agent_capability_search -> agent_workflow_describe -> tools/list";

export type AgentScope = "company" | "workspace" | "legal-group" | "system";
export type WorkflowBoundary = "read" | "dry-run" | "review" | "approval" | "apply" | "irreversible" | "destructive";
export type OperationSafety = "read" | "write" | "destructive";
/** The only retry contracts exposed to agents.  A write never becomes safe
 * merely because it accepts an arbitrary input field. */
export type RetryClass = "safe-read" | "key-idempotent" | "natural-idempotent" | "external-provider-reconciled" | "unsafe-read-back";

export type OperationReference =
  | { surface: "mcp"; name: string }
  | { surface: "cli"; key: string }
  | { surface: "http"; method: string; pattern: string };

export type AgentCapability = {
  id: string;
  title: string;
  purpose: string;
  domain: string;
  outcomes: string[];
  keywords: string[];
  scope: AgentScope;
  supportStatus: "supported" | "partial";
  maturity: "stable" | "limited";
  workflowIds: string[];
  canonicalState: string[];
  unsupportedBoundaries: string[];
};

export type AgentWorkflowStep = {
  id: string;
  dependsOn: string[];
  condition?: string;
  boundary: WorkflowBoundary;
  operation: OperationReference;
  purpose: string;
  prerequisites: string[];
  inputIdentities: string[];
  outputIdentities: string[];
  expectedSafety: OperationSafety;
  expectedIdempotent: boolean;
  requiresActor: boolean;
  requiresConfirmation: boolean;
  requiredArguments?: string[];
  retryClass: RetryClass;
  uncertainOutcomeReadBack?: OperationReference;
  canonicalRecords: string[];
};

export type AgentWorkflow = {
  id: string;
  capabilityId: string;
  title: string;
  intendedOutcome: string;
  nonGoals: string[];
  prerequisites: string[];
  steps: AgentWorkflowStep[];
  blockers: Array<{ code: string; meaning: string }>;
  recovery: string[];
  stopConditions: string[];
  relatedWorkflowIds: string[];
  alternatives: string[];
  unsupportedBoundaries: string[];
};

type StepInput = Omit<AgentWorkflowStep, "dependsOn" | "prerequisites" | "inputIdentities" | "outputIdentities" | "canonicalRecords"> & Partial<Pick<AgentWorkflowStep, "dependsOn" | "prerequisites" | "inputIdentities" | "outputIdentities" | "canonicalRecords">>;

const mcp = (name: string): OperationReference => ({ surface: "mcp", name });
const cli = (key: string): OperationReference => ({ surface: "cli", key });
const http = (method: string, pattern: string): OperationReference => ({ surface: "http", method, pattern });

function step(input: StepInput): AgentWorkflowStep {
  return { dependsOn: [], prerequisites: [], inputIdentities: [], outputIdentities: [], canonicalRecords: [], ...input };
}

function read(id: string, operation: OperationReference, purpose: string, options: Partial<StepInput> = {}): AgentWorkflowStep {
  return step({ id, operation, purpose, boundary: "read", expectedSafety: "read", expectedIdempotent: true, requiresActor: false, requiresConfirmation: false, retryClass: "safe-read", ...options });
}

function write(id: string, operation: OperationReference, purpose: string, options: Partial<StepInput> = {}): AgentWorkflowStep {
  return step({ id, operation, purpose, boundary: "apply", expectedSafety: "write", expectedIdempotent: false, requiresActor: true, requiresConfirmation: true, retryClass: "unsafe-read-back", ...options });
}

function workflow(input: Pick<AgentWorkflow, "id" | "capabilityId" | "title" | "intendedOutcome" | "steps"> & Partial<Omit<AgentWorkflow, "id" | "capabilityId" | "title" | "intendedOutcome" | "steps">>): AgentWorkflow {
  return {
    nonGoals: ["Discovery is not authorization and never executes the workflow."],
    prerequisites: ["Call meta_about and verify build, rules and catalogue identity.", "Resolve every referenced operation in the live surface before mutation.", "Use an explicit company, workspace or legal-group identity where required."],
    blockers: [
      { code: "INPUT_VALIDATION", meaning: "Correct the named schema field before retry." },
      { code: "CONFIRM_OR_ACTOR_REQUIRED", meaning: "The write lacks confirmation, actor attribution or permission." },
      { code: "BUSINESS_PRECONDITION", meaning: "Evidence, period, backup, review or accounting rules block the operation." },
    ],
    recovery: ["Stop at the failing boundary and inspect the structured error or MCP -32602 response.", "Repair only the named precondition, read canonical state again, and use correction/reversal rather than deleting ledger evidence."],
    stopConditions: ["A required human approval is absent.", "A referenced operation is not live on the declared surface.", "The requested outcome crosses an unsupported boundary."],
    relatedWorkflowIds: [], alternatives: [], unsupportedBoundaries: [], ...input,
  };
}

export const AGENT_WORKFLOWS: readonly AgentWorkflow[] = [
  workflow({ id: "company-workspace-setup", capabilityId: "company-workspace", title: "Company and workspace setup", intendedOutcome: "Create or select a company and verify its canonical profile before accounting work.", steps: [
    write("initialize-company", cli("init"), "Create a local company root when no workspace company exists.", { requiresActor: false, requiresConfirmation: false, prerequisites: ["CLI-only: choose an explicit new company path."], outputIdentities: ["company root"], canonicalRecords: ["company database", "company profile"] }),
    write("add-workspace-company", mcp("company_add"), "Add a company to an existing workspace.", { condition: "Use instead of initialize-company for an existing workspace.", outputIdentities: ["company slug"], canonicalRecords: ["workspace manifest"] }),
    read("list-companies", mcp("portfolio_overview"), "List only canonical registered live companies; sibling copies are never inferred from directories.", { dependsOn: ["initialize-company|add-workspace-company"], outputIdentities: ["visible company slugs"] }),
    read("read-profile", mcp("company_profile_get"), "Verify the selected company's profile.", { dependsOn: ["list-companies"], inputIdentities: ["company slug/path"], canonicalRecords: ["company profile"] }),
  ], unsupportedBoundaries: ["MCP does not initialize an arbitrary host path; CLI init is explicitly CLI-only.", "Discovery never exposes companies outside the caller's workspace access."] }),
  workflow({ id: "local-service-principal-lifecycle", capabilityId: "company-workspace", title: "Local service-principal lifecycle", intendedOutcome: "Bootstrap, rotate or revoke a local authenticated service credential without treating an audit actor as authorization.", steps: [
    write("bootstrap", cli("workspace-access bootstrap-local-service"), "Create exactly one local Better Auth service credential with an explicit minimum role and show it once.", { requiredArguments: ["workspace", "company", "display-name", "company-role", "auth-secret-file", "confirm"], inputIdentities: ["company slug", "company role"], outputIdentities: ["serviceAccountId", "credentialId"], canonicalRecords: ["service principal", "workspace access event", "company membership event", "credential audit"] }),
    write("rotate", cli("workspace-access local-service-rotate"), "Rotate the exact credential after recording its id; the replacement secret is shown once.", { condition: "Use only to replace an existing credential.", requiredArguments: ["workspace", "company", "service-account-id", "credential-id", "auth-secret-file", "confirm"], inputIdentities: ["serviceAccountId", "credentialId"], outputIdentities: ["replacement credentialId"], canonicalRecords: ["service credential rotation audit"] }),
    write("revoke", cli("workspace-access local-service-revoke"), "Disable the exact credential append-only when access must end.", { condition: "Use instead of deletion.", requiredArguments: ["workspace", "company", "service-account-id", "credential-id", "auth-secret-file", "confirm"], inputIdentities: ["serviceAccountId", "credentialId"], canonicalRecords: ["service credential revocation audit"] }),
  ], unsupportedBoundaries: ["An actor is audit attribution only and never a credential or permission grant.", "A service principal receives no access outside its active workspace and company memberships.", "Local bootstrap never creates a hosted browser user or an implicit owner token."] }),
  workflow({ id: "document-mail-intake", capabilityId: "document-intake", title: "Document and mail intake", intendedOutcome: "Store source evidence and review extraction without silently approving or posting it.", steps: [
    write("ingest-document", mcp("documents_ingest"), "Ingest supplied source evidence.", { outputIdentities: ["documentId", "documentNo", "sha256"], canonicalRecords: ["documents", "document originals"] }),
    write("ingest-mail", mcp("mail_intake_ingest"), "Ingest an explicitly selected mail attachment.", { condition: "Use for mail intake instead of ingest-document.", expectedIdempotent: true, retryClass: "external-provider-reconciled", outputIdentities: ["documentId"], canonicalRecords: ["documents", "mail intake audit"] }),
    write("poll-imap", mcp("imap_intake_poll"), "Poll configured IMAP intake and ingest accepted attachments.", { condition: "Use only when IMAP is configured and external access is intended.", expectedIdempotent: true, retryClass: "external-provider-reconciled", outputIdentities: ["intake batch identity"], canonicalRecords: ["documents", "mail intake audit"] }),
    read("list-documents", mcp("documents_list"), "Read back canonical document state.", { dependsOn: ["ingest-document|ingest-mail|poll-imap"], outputIdentities: ["documentId"] }),
    read("review-extraction", mcp("documents_invoice_extraction"), "Review cited invoice extraction where available.", { dependsOn: ["list-documents"], inputIdentities: ["documentId"] }),
  ], unsupportedBoundaries: ["Extraction is evidence for review, not approval or automatic posting.", "The catalogue contains no mailbox credentials or company routing."] }),
  workflow({ id: "incomplete-purchase-evidence-review", capabilityId: "document-intake", title: "Incomplete standard purchase evidence review", intendedOutcome: "Preserve an incomplete standard purchase invoice truthfully and only use input VAT after a source-bound review establishes the material conditions.", steps: [
    write("ingest-source", mcp("documents_ingest"), "Ingest the original invoice with incompleteStandardPurchaseInvoice:true only when the buyer fields are actually absent in the source.", { outputIdentities: ["documentId", "sha256"], canonicalRecords: ["documents", "immutable source bytes"] }),
    write("record-context", mcp("documents_set_company_context"), "Record source reference and business-use evidence append-only. This binds the original and metadata hashes but never rewrites invoice recipient facts or approves VAT.", { dependsOn: ["ingest-source"], expectedIdempotent: true, retryClass: "natural-idempotent", inputIdentities: ["documentId"], canonicalRecords: ["document company contexts", "audit log"] }),
    write("review-material-evidence", mcp("documents_review_purchase_vat_evidence"), "Bind an exact matching outgoing company payment and a SHA-256-identified business-use source. A reviewer must explicitly confirm this formal-deficiency-only decision.", { dependsOn: ["record-context"], expectedIdempotent: true, retryClass: "natural-idempotent", inputIdentities: ["documentId", "bankTransactionId", "businessEvidenceSha256"], canonicalRecords: ["purchase VAT evidence review", "audit log"] }),
    read("vat-preflight", mcp("expense_vat_preflight"), "Read the actual booking/VAT gate after the evidence review.", { dependsOn: ["review-material-evidence"], boundary: "dry-run", inputIdentities: ["documentId"], canonicalRecords: ["purchase VAT preflight"] }),
  ], unsupportedBoundaries: ["Company context is business-attribution evidence, not a correction to the issuer's invoice.", "This workflow never infers a buyer CVR, treats a missing address as a simplified invoice, or grants VAT deduction automatically.", "The review fails closed if supplier identity, invoice VAT, company payment, source hashes or business use are missing or change."] }),
  workflow({ id: "non-eu-reverse-charge-evidence-review", capabilityId: "document-intake", title: "Non-EU reverse-charge material review", intendedOutcome: "Keep a formally deficient non-EU service invoice immutable and permit reverse charge only after a reviewer binds verifiable material evidence.", steps: [
    write("ingest-source", mcp("documents_ingest"), "Ingest the source invoice with an explicit non-EU supplier country and typed non_eu identity; an observed OSS or IE VAT identifier is not EU establishment evidence.", { outputIdentities:["documentId","sha256"], canonicalRecords:["documents","immutable source bytes"] }),
    write("review-material-evidence", mcp("documents_review_non_eu_reverse_charge_evidence"), "Record supplier establishment, actual Danish buyer, service/place/use, period, deduction percentage and hashes for every supporting source. confirm:true is required.", { dependsOn:["ingest-source"], expectedIdempotent:true, retryClass:"natural-idempotent", inputIdentities:["documentId","supplierEvidenceSha256","buyerEvidenceSha256","serviceEvidenceSha256"], canonicalRecords:["non-EU reverse-charge review","audit log"] }),
    read("vat-preflight", mcp("expense_vat_preflight"), "Read the actual booking gate after review; it performs no mutation.", { dependsOn:["review-material-evidence"], boundary:"dry-run", inputIdentities:["documentId"], canonicalRecords:["purchase VAT preflight"] }),
  ], unsupportedBoundaries:["No review rewrites invoice metadata or uses actor identity as evidence.", "Foreign/local VAT charged by the supplier is rejected, never relabelled as Danish reverse charge.", "The workflow does not infer EU establishment from OSS/IE VAT or a tax identifier alone."] }),
  workflow({ id: "external-payroll-evidence-journal", capabilityId: "document-intake", title: "External payroll evidence and settlement", intendedOutcome: "Record an external payroll report as source-linked, no-VAT evidence, then separately post the reviewed accrual and the bank settlement.", steps: [
    write("ingest-report", mcp("documents_ingest"), "Ingest external_accounting_evidence with payroll category, report period, external reference, balanced totals and vatAmount:0.", { outputIdentities: ["documentId", "sha256"], canonicalRecords: ["external payroll evidence", "immutable source bytes"] }),
    read("review-report", mcp("documents_list"), "Read the source-linked report back; verify issuer, reported company, period, reference and totals before any journal.", { dependsOn: ["ingest-report"], inputIdentities: ["documentId"], canonicalRecords: ["documents"] }),
    read("preview-accrual", mcp("journal_dry_run"), "Preview the balanced accrual lines against approved wage and statutory-liability accounts. It writes nothing and never derives payroll.", { dependsOn: ["review-report"], boundary: "dry-run", inputIdentities: ["documentId"], canonicalRecords: ["journal preview"] }),
    write("post-accrual", mcp("journal_post"), "Post the explicitly reviewed accrual with the payroll document link and idempotency key.", { dependsOn: ["preview-accrual"], retryClass: "key-idempotent", inputIdentities: ["documentId", "idempotencyKey"], canonicalRecords: ["journal entries", "statutory payroll liabilities", "audit log"] }),
    read("preview-settlement", mcp("journal_dry_run"), "Preview the later net-pay bank settlement against the imported bank evidence; it is a separate journal.", { dependsOn: ["post-accrual"], boundary: "dry-run", canonicalRecords: ["journal preview", "bank transaction"] }),
    write("post-settlement", mcp("journal_post"), "Post the reviewed bank settlement with its own idempotency key and read back the resulting liability balance.", { dependsOn: ["preview-settlement"], retryClass: "key-idempotent", inputIdentities: ["idempotencyKey"], canonicalRecords: ["journal entries", "bank reconciliation", "audit log"] }),
  ], unsupportedBoundaries: ["Rentemester does not calculate payroll, create salary payments, infer tax rates or transmit payroll filings.", "The report is external evidence; payroll-specific account selection and amounts require explicit human review.", "VAT remains zero for this evidence type."] }),
  workflow({ id: "non-cash-balance-correction", capabilityId: "non-cash-balance-corrections", title: "Non-cash balance correction", intendedOutcome: "Record one documented DKK balance-only correction without inventing a bank movement or changing VAT.", steps: [
    write("ingest-evidence", mcp("documents_ingest"), "Ingest a non_cash_balance_correction internal voucher with rationale, date, amount and DKK currency.", { outputIdentities: ["documentId", "sha256"], canonicalRecords: ["documents", "non-cash balance correction evidence"] }),
    read("review-evidence", mcp("documents_list"), "Read back the voucher kind, rationale and actor provenance before journal planning.", { dependsOn: ["ingest-evidence"], inputIdentities: ["documentId"], canonicalRecords: ["documents", "non-cash balance correction evidence"] }),
    read("dry-run", mcp("journal_dry_run"), "Preview the exact two-sided DKK balance-only journal; it writes nothing.", { dependsOn: ["review-evidence"], inputIdentities: ["documentId"], canonicalRecords: ["journal preview"] }),
    write("post", mcp("journal_post"), "Post the reviewed journal with the same documentId and explicit confirmation.", { dependsOn: ["dry-run"], retryClass: "key-idempotent", inputIdentities: ["documentId", "idempotencyKey"], canonicalRecords: ["journal entries", "non-cash balance correction postings", "audit log"] }),
    read("readback", mcp("journal_list"), "Read back the posted journal and use documents_list for the durable voucher link.", { dependsOn: ["post"], inputIdentities: ["documentId"] }),
  ], unsupportedBoundaries: ["Only DKK balance corrections are supported; foreign-currency correction vouchers are rejected.", "Bank, VAT, income, expense, debtor and creditor accounts are rejected.", "This workflow never creates a bank transaction, VAT adjustment, payable or sales record."] }),
  workflow({ id: "legacy-opening-creditor-reclassification", capabilityId: "non-cash-balance-corrections", title: "Legacy opening creditor reclassification", intendedOutcome: "Reclassify an unexplained DKK creditor primobalance without creating a payable or altering historical journals.", steps: [
    read("inspect-opening",mcp("journal_list"),"Identify the exact posted primobalance journal and creditor line.",{outputIdentities:["openingJournalEntryId","openingJournalLineId"],canonicalRecords:["opening balance","journal lines"]}),
    write("ingest-evidence",mcp("documents_ingest"),"Ingest a legacy_opening_creditor_reclassification voucher with exact primobalance IDs.",{dependsOn:["inspect-opening"],outputIdentities:["documentId","sha256"],canonicalRecords:["documents","legacy opening creditor reclassification evidence"]}),
    read("dry-run",mcp("journal_dry_run"),"Preview exactly two DKK balance-sheet lines; no write occurs.",{dependsOn:["ingest-evidence"],inputIdentities:["documentId"],canonicalRecords:["journal preview"]}),
    write("post",mcp("journal_post"),"Post the reviewed journal; an exact retry is idempotent.",{dependsOn:["dry-run"],retryClass:"key-idempotent",inputIdentities:["documentId","idempotencyKey"],canonicalRecords:["journal entries","non-cash balance correction postings","audit log"]}),
    read("readback",mcp("journal_list"),"Read back the correction and creditor balance effect.",{dependsOn:["post"],inputIdentities:["documentId"]}),
  ], unsupportedBoundaries:["This never handles ordinary supplier invoices, active creditor balances, payable payments or bank settlements.","Only a documented still-unexplained posted primobalance creditor line is eligible.","Bank, VAT, income, expense and debtor accounts remain forbidden; historical journals are never changed."] }),
  workflow({ id: "workspace-document-inbox", capabilityId: "workspace-document-inbox", title: "Workspace document inbox routing", intendedOutcome: "Store one immutable source outside every ledger, resolve only authorized deterministic candidates, then hand it off once to the selected company pipeline.", steps: [
    read("list", http("GET", "/api/companies/:slug/workspace-inbox"), "List only sources visible through the chosen company access anchor."),
    write("ingest", http("POST", "/api/companies/:slug/workspace-inbox"), "Store immutable source bytes and filtered routing evidence without opening a company ledger; on a lost response, inspect before retrying.", { inputIdentities:["idempotencyKey"], canonicalRecords:["workspace inbox source", "workspace inbox routing events"] }),
    read("inspect", http("GET", "/api/companies/:slug/workspace-inbox/:sourceId"), "Inspect the source, redacted candidates, exception and current assignment.", { dependsOn:["ingest"] }),
    write("assign", http("POST", "/api/companies/:slug/workspace-inbox/:sourceId/assign"), "Explicitly approve one authorized legal entity; ambiguous content is never silently routed.", { dependsOn:["inspect"], boundary:"approval", canonicalRecords:["workspace inbox assignment"] }),
    write("complete", http("POST", "/api/companies/:slug/workspace-inbox/:sourceId/complete"), "Hand off exactly once to canonical per-company document ingest and read back the durable assignment.", { dependsOn:["assign"], boundary:"irreversible", uncertainOutcomeReadBack:http("GET", "/api/companies/:slug/workspace-inbox/:sourceId"), canonicalRecords:["company document", "workspace inbox handoff event"] }),
  ], unsupportedBoundaries:["Workspace inbox never posts, calculates VAT or maintains a workspace ledger.", "Hidden candidate companies and their metadata are filtered before output, ordering and exception rendering.", "A linked or booked document is corrected through its company controls, never silently reassigned."] }),
  workflow({ id: "bank-reconciliation-batch", capabilityId: "bank-bookkeeping", title: "Bank import, matching and bookkeeping batch", intendedOutcome: "Import bank activity, inspect matches and apply only a hash-bound reviewed bookkeeping batch.", steps: [
    write("import-bank", mcp("bank_import"), "Import bank rows with duplicate protection. For same-date running balances, use a profile or explicit statementOrder; an unprovable order stays ambiguous rather than selecting a row id.", { expectedIdempotent: true, retryClass: "natural-idempotent", outputIdentities: ["importBatchId", "bankTransactionIds"], canonicalRecords: ["bank_transactions", "bank statement source-order evidence"] }),
    read("suggest-matches", mcp("bank_suggest_matches"), "Generate read-only matching suggestions.", { dependsOn: ["import-bank"] }),
    read("reconciliation-report", mcp("reconcile_bank"), "Produce the read-only reconciliation report; this does not confirm matches.", { dependsOn: ["import-bank"] }),
    read("batch-plan", mcp("bookkeeping_batch_plan"), "Produce a deterministic plan and hash without changing durable state.", { dependsOn: ["suggest-matches"], boundary: "dry-run", expectedIdempotent: true, retryClass: "safe-read", outputIdentities: ["planHash"] }),
    write("batch-persist", mcp("bookkeeping_batch_persist"), "Persist the reviewed plan and hash; this does not approve or apply it.", { dependsOn: ["batch-plan"], boundary: "dry-run", expectedIdempotent: true, retryClass: "natural-idempotent", inputIdentities: ["runKey"], outputIdentities: ["runId", "planHash"] }),
    write("batch-approve", mcp("bookkeeping_batch_approve"), "Bind an authorised reviewer, time and exact hash to the persisted run.", { dependsOn: ["batch-persist"], boundary: "approval", expectedIdempotent: true, retryClass: "natural-idempotent", inputIdentities: ["runId", "planHash"] }),
    write("batch-apply", mcp("bookkeeping_batch_apply"), "Apply or resume the exact approved run without replanning.", { dependsOn: ["batch-approve"], boundary: "irreversible", retryClass: "natural-idempotent", expectedIdempotent: true, inputIdentities: ["runId", "planHash"], outputIdentities: ["runId", "journalEntryIds"], uncertainOutcomeReadBack: mcp("bookkeeping_batch_status"), canonicalRecords: ["bookkeeping batch runs", "journal entries", "bank reconciliations"] }),
    read("read-bank-state", mcp("bank_list"), "Read back imported and reconciled bank state.", { dependsOn: ["batch-apply"] }),
  ], unsupportedBoundaries: ["reconcile_bank is a report, not an apply operation.", "Suggested or human-review items are never auto-approved."] }),
  workflow({ id:"bank-reconciliation-correction", capabilityId:"bank-bookkeeping", title:"Correct a reversed bank reconciliation", intendedOutcome:"Replace exactly one reversed reconciliation without changing historical journals or links.", steps:[
    read("inspect",mcp("bank_reconciliation_correction_plan"),"Build and inspect the deterministic correction plan, including its current reconciliation identity and plan hash.",{boundary:"dry-run",outputIdentities:["expectedReconciliationId","planHash"],canonicalRecords:["bank reconciliation projection","journal reversal"]}),
    write("apply",mcp("bank_reconciliation_correction_apply"),"Atomically supersede only the exact reviewed reconciliation with the replacement journal; a matching idempotency key replays its durable result.",{dependsOn:["inspect"],boundary:"irreversible",expectedIdempotent:false,retryClass:"key-idempotent",inputIdentities:["expectedReconciliationId","planHash","idempotencyKey"],canonicalRecords:["bank reconciliation correction events","audit log"]}),
  ], relatedWorkflowIds:["bank-reconciliation-batch"], unsupportedBoundaries:["The correction never edits journals, reversals, hashes or legacy reconciliation links.","A current active old journal must be reversed before a correction can be planned."] }),
  workflow({ id:"direct-bank-purchase-payable-correction",capabilityId:"supplier-purchases",title:"Correct a direct-bank purchase into a payable",intendedOutcome:"Preserve immutable purchase evidence while moving the accounting effect into the canonical payable lifecycle.",steps:[
    read("plan",mcp("direct_bank_purchase_payable_correction_plan"),"Inspect the deterministic hash-bound conversion plan.",{boundary:"dry-run",outputIdentities:["planHash"]}),
    write("apply",mcp("direct_bank_purchase_payable_correction_apply"),"Atomically reverse the original posting, register the payable and settle it on the bank date.",{dependsOn:["plan"],boundary:"irreversible",retryClass:"key-idempotent",inputIdentities:["planHash","idempotencyKey"],canonicalRecords:["journal entries","payables","payable payments","audit log"]}),
  ],relatedWorkflowIds:["supplier-payable-handling","bank-reconciliation-correction"],unsupportedBoundaries:["The workflow never changes a document hash or metadata.","Journal history is reversed and appended, never rewritten."]}),
  workflow({id:"legacy-bank-payable-backfill",capabilityId:"supplier-purchases",title:"Adopt explicit legacy bank and payable evidence",intendedOutcome:"Bind only an unassigned legacy bank account and append a payable/payment only for exact reviewed source IDs.",steps:[
    read("plan-bank-binding",mcp("bank_legacy_binding_plan"),"Verify the NULL-only bank binding, source endpoint, roles, cutoff balances and heads.",{boundary:"dry-run",outputIdentities:["planHash"]}),
    write("apply-bank-binding",mcp("bank_legacy_binding_apply"),"Apply only the exact reviewed NULL-to-account binding.",{dependsOn:["plan-bank-binding"],boundary:"approval",expectedIdempotent:true,retryClass:"natural-idempotent",inputIdentities:["planHash","idempotencyKey"],canonicalRecords:["legacy bank binding","audit log"]}),
    read("plan-payable-payment",mcp("payable_legacy_backfill_plan"),"Verify exactly named purchase journal, payment journal, document and bank transaction; no amount matching is performed.",{boundary:"dry-run",outputIdentities:["planHash"]}),
    write("apply-payable-payment",mcp("payable_legacy_backfill_apply"),"Append canonical payable/payment and immutable backfill evidence only.",{dependsOn:["plan-payable-payment"],boundary:"approval",expectedIdempotent:true,retryClass:"natural-idempotent",inputIdentities:["planHash","idempotencyKey"],canonicalRecords:["payables","payable payments","legacy backfill audit"]}),
  ],unsupportedBoundaries:["No amount-only matching, journal posting, document rewrite, bank rewrite or remapping is supported.","Any pre-existing binding, payable, payment, reversal or conflicting reconciliation fails closed."]}),
  workflow({ id: "bookkeeping-workbench", capabilityId: "bank-bookkeeping", title: "Bookkeeping workbench", intendedOutcome: "Read one canonical queue of unresolved bank work, then use the existing reviewed batch workflow for every write.", steps: [
    read("read-workbench",mcp("bookkeeping_workbench"),"Read the deterministic unresolved bank population, canonical document-party resolution and account/VAT/dimension evidence. Use returned drilldowns to inspect source evidence before the separate batch plan.",{inputIdentities:["company","from","to","bankAccountId","partyId","documentQuality","account","vatTreatment","dimension"],outputIdentities:["bankTransactionId","sourceHash","planHash","population.blockers"],canonicalRecords:["bank transactions","bank journal reconciliations","current document party links","document party resolution events","bookkeeping batch runs","period-close readiness"]}),
    read("review-plan",mcp("bookkeeping_batch_plan"),"Revalidate the exact batch plan before any persist or approval.",{dependsOn:["read-workbench"],boundary:"dry-run",outputIdentities:["planHash"]}),
    write("persist-reviewed-plan",mcp("bookkeeping_batch_persist"),"Persist a reviewed plan using the existing batch contract.",{dependsOn:["review-plan"],boundary:"dry-run",expectedIdempotent:true,retryClass:"natural-idempotent",inputIdentities:["runKey","planHash"]}),
    write("approve-reviewed-plan",mcp("bookkeeping_batch_approve"),"Approve the exact persisted hash with a distinct stable principal.",{dependsOn:["persist-reviewed-plan"],boundary:"approval",expectedIdempotent:true,retryClass:"natural-idempotent",inputIdentities:["runId","planHash"]}),
    write("apply-reviewed-plan",mcp("bookkeeping_batch_apply"),"Apply or resume only the exact approved run and hash.",{dependsOn:["approve-reviewed-plan"],boundary:"irreversible",expectedIdempotent:true,retryClass:"natural-idempotent",inputIdentities:["runId","planHash"],uncertainOutcomeReadBack:mcp("bookkeeping_batch_status")}),
  ],relatedWorkflowIds:["bank-reconciliation-batch"],unsupportedBoundaries:["The workbench never creates a reconciliation, task database or posting.","A row is not dismissible: only canonical reconciliation, correction or reviewed batch effects change completion."]}),
  workflow({ id: "supplier-expense-booking", capabilityId: "supplier-purchases", title: "Supplier expense booking", intendedOutcome: "Book a documented supplier expense against a bank transaction with the correct VAT treatment.", steps: [
    read("review-document", mcp("documents_list"), "Select the ingested supplier document."),
    read("vat-preflight", mcp("expense_vat_preflight"), "Validate supplier identity, VAT evidence and treatment.", { dependsOn: ["review-document"], boundary: "dry-run" }),
    write("book-expense", mcp("expense_book"), "Post the reviewed expense and bank reconciliation.", { dependsOn: ["vat-preflight"], boundary: "irreversible", retryClass: "key-idempotent", inputIdentities: ["documentId", "bankTransactionId"], uncertainOutcomeReadBack: mcp("journal_list"), canonicalRecords: ["journal entries", "bank reconciliations", "document posting link"] }),
    read("verify-posting", mcp("journal_list"), "Read back the posting.", { dependsOn: ["book-expense"] }),
  ], relatedWorkflowIds: ["supplier-payable-handling", "vat-preparation"] }),
  workflow({ id:"purchase-case-lifecycle", capabilityId:"supplier-purchases", title:"Provisional purchase case lifecycle", intendedOutcome:"Record a source-bound provisional purchase case, review exact evidence, then continue only through an existing booking flow.", steps:[
    write("create",mcp("purchase_case_create"),"Create one append-only case bound to an existing document, bank transaction or payable. This never posts or changes VAT.",{boundary:"review",retryClass:"key-idempotent",inputIdentities:["source.kind","source.id","idempotencyKey"],outputIdentities:["caseId","sourceFingerprint"],canonicalRecords:["purchase case events","audit log"]}),
    read("inspect",mcp("purchase_case_get"),"Read the derived accounting and VAT evidence state without mutation.",{dependsOn:["create"],inputIdentities:["caseId"]}),
    write("review",mcp("purchase_case_review"),"Append only a review of the exact case version and source fingerprint.",{dependsOn:["inspect"],boundary:"review",retryClass:"key-idempotent",inputIdentities:["caseId","expectedVersion","expectedSourceFingerprint","idempotencyKey"],uncertainOutcomeReadBack:mcp("purchase_case_get"),canonicalRecords:["purchase case events","audit log"]}),
    read("readback",mcp("purchase_case_get"),"Read the current case after a retry or review.",{dependsOn:["review"],inputIdentities:["caseId"]}),
  ],relatedWorkflowIds:["supplier-expense-booking","supplier-payable-handling"],unsupportedBoundaries:["A purchase case is not a ledger, journal, draft, VAT approval or payable.","Booking remains in the existing reviewed supplier expense or payable workflow.","Changing source evidence makes a previous review stale."]}),
  workflow({ id:"purchase-case-group-review", capabilityId:"supplier-purchases", title:"Grouped purchase-evidence review", intendedOutcome:"Inspect grouped human needs and atomically review only exact, compatible unresolved purchase cases.", steps:[
    read("overview",mcp("purchase_overview"),"Read the source-based overview and exact grouped case versions and fingerprints.",{inputIdentities:["from","to"],outputIdentities:["groups.members.caseId","groups.members.version","groups.members.sourceFingerprint"],canonicalRecords:["purchase case events","documents","bank transactions","payables"]}),
    write("group-review",mcp("purchase_case_group_review"),"Preflight the complete exact selection, then append one shared group event and one review per case. A stale or incompatible member rejects the whole selection.",{dependsOn:["overview"],boundary:"review",retryClass:"key-idempotent",inputIdentities:["members.caseId","members.expectedVersion","members.expectedSourceFingerprint","idempotencyKey"],uncertainOutcomeReadBack:mcp("purchase_overview"),canonicalRecords:["purchase case group events","purchase case events","audit log"]}),
    read("readback",mcp("purchase_overview"),"Read the current grouped needs after the exact review or a retry.",{dependsOn:["group-review"]}),
  ],relatedWorkflowIds:["purchase-case-lifecycle","supplier-expense-booking"],unsupportedBoundaries:["The grouped review never posts, changes VAT state or partially applies a selection.","Unposted cases continue only through the existing booking workflows."]}),
  workflow({ id: "supplier-payable-handling", capabilityId: "supplier-purchases", title: "Supplier payable handling", intendedOutcome: "Register a supplier invoice as an open payable and record its later bank payment.", steps: [
    write("register-payable", mcp("payable_register"), "Register reviewed evidence as a payable.", { boundary: "irreversible", retryClass: "key-idempotent", outputIdentities: ["payableId", "journalEntryId"], canonicalRecords: ["payables", "journal entries"] }),
    read("list-payables", mcp("payable_list"), "Read due/open state.", { dependsOn: ["register-payable"] }),
    write("pay-payable", mcp("payable_pay"), "Match the selected bank payment.", { dependsOn: ["list-payables"], boundary: "irreversible", retryClass: "key-idempotent", inputIdentities: ["payableId", "bankTransactionId"], uncertainOutcomeReadBack: mcp("payable_list"), canonicalRecords: ["payable payments", "bank reconciliations", "journal entries"] }),
    read("verify-payable", mcp("payable_list"), "Read back the payable balance.", { dependsOn: ["pay-payable"] }),
  ], relatedWorkflowIds: ["supplier-expense-booking"] }),
  workflow({ id:"supplier-commitment-forecast", capabilityId:"supplier-commitments", title:"Supplier commitment and 13-week cash forecast", intendedOutcome:"Review recurring supplier evidence and inspect a source-linked weekly cash forecast without creating a payable or payment.", steps:[
    read("plan-commitment",mcp("supplier_commitment_plan"),"Validate source-linked planning evidence without mutation.",{boundary:"dry-run",expectedIdempotent:true,retryClass:"safe-read",outputIdentities:["payloadHash"]}),
    write("apply-commitment",mcp("supplier_commitment_apply"),"Record the exact reviewed commitment hash; it never posts, pays or sends.",{dependsOn:["plan-commitment"],boundary:"approval",expectedIdempotent:false,retryClass:"unsafe-read-back",inputIdentities:["payloadHash"],uncertainOutcomeReadBack:mcp("supplier_commitment_list"),canonicalRecords:["supplier commitment events"]}),
    read("forecast",mcp("liquidity_forecast_13_week"),"Read source buckets, excluded currencies and lowest cash point.",{dependsOn:["apply-commitment"],retryClass:"safe-read"}),
    read("verify",mcp("supplier_commitment_list"),"Read back active source hashes.",{dependsOn:["forecast"]}),
  ], unsupportedBoundaries:["A recurring bank pattern is only a proposal, never a contract.","This workflow never creates invoices, payables, journals, payments, cancellations or supplier messages.","Foreign currency is excluded unless an explicit dated FX source exists."] }),
  workflow({ id: "customer-invoice-lifecycle", capabilityId: "customer-invoicing", title: "Customer and invoice lifecycle", intendedOutcome: "Create a customer, issue and post an invoice, then handle delivery, payment, reminder or credit-note branches.", steps: [
    write("create-customer", mcp("customer_create"), "Create canonical customer master data.", { outputIdentities: ["customerId"], canonicalRecords: ["customers"] }),
    read("validate-invoice", mcp("invoice_validate"), "Validate the invoice payload.", { dependsOn: ["create-customer"], boundary: "dry-run" }),
    write("issue-invoice", mcp("invoice_issue"), "Issue immutable invoice evidence.", { dependsOn: ["validate-invoice"], outputIdentities: ["invoiceNumber", "documentId"], uncertainOutcomeReadBack: mcp("invoice_find"), canonicalRecords: ["issued invoices", "invoice documents"] }),
    write("post-invoice", mcp("invoice_post"), "Post the issued invoice.", { dependsOn: ["issue-invoice"], boundary: "irreversible", uncertainOutcomeReadBack: mcp("invoice_status"), canonicalRecords: ["journal entries", "invoice open balance"] }),
    write("send-email", mcp("invoice_send_email"), "Send the issued invoice by configured email.", { dependsOn: ["issue-invoice"], condition: "Optional delivery branch. SMTP has no provider reconciliation contract: read canonical delivery evidence before any retry.", retryClass: "unsafe-read-back", canonicalRecords: ["email delivery evidence"] }),
    write("record-payment", mcp("invoice_settle_bank"), "Match a customer bank payment.", { dependsOn: ["post-invoice"], condition: "Payment branch.", boundary: "irreversible", uncertainOutcomeReadBack: mcp("invoice_status"), canonicalRecords: ["invoice payments", "bank reconciliations", "journal entries"] }),
    write("send-reminder", mcp("invoice_remind"), "Create an eligible overdue reminder.", { dependsOn: ["post-invoice"], condition: "Overdue branch.", canonicalRecords: ["invoice reminders"] }),
    write("credit-note", mcp("invoice_credit_note"), "Correct an issued invoice by credit note.", { dependsOn: ["issue-invoice"], condition: "Correction branch.", boundary: "irreversible", uncertainOutcomeReadBack: mcp("invoice_status"), canonicalRecords: ["credit notes", "journal entries"] }),
    read("invoice-status", mcp("invoice_status"), "Read back invoice, claim and payment state.", { dependsOn: ["post-invoice"] }),
  ], unsupportedBoundaries: ["Issue does not imply delivery or payment.", "No external send is retried blindly after uncertainty."] }),
  workflow({ id: "vat-preparation", capabilityId: "vat", title: "Domestic purchase VAT and period preparation", intendedOutcome: "Validate purchase VAT evidence, post supported treatments and prepare reports without filing externally.", steps: [
    read("purchase-preflight", mcp("expense_vat_preflight"), "Validate domestic supplier and line-level VAT evidence.", { boundary: "dry-run" }),
    write("post-domestic-purchase", mcp("expense_book"), "Post the reviewed domestic treatment.", { dependsOn: ["purchase-preflight"], condition: "Domestic branch.", boundary: "irreversible", retryClass: "key-idempotent", uncertainOutcomeReadBack: mcp("vat_report"), canonicalRecords: ["journal entries with VAT codes"] }),
    write("post-reverse-charge", mcp("vat_post_eu_service_purchase"), "Post a supported EU-service reverse charge.", { dependsOn: ["purchase-preflight"], condition: "Reverse-charge branch.", boundary: "irreversible", uncertainOutcomeReadBack: mcp("vat_report"), canonicalRecords: ["reverse-charge journal entries"] }),
    read("vat-report", mcp("vat_report"), "Prepare the VAT period report.", { dependsOn: ["post-domestic-purchase|post-reverse-charge"] }),
    read("eu-sales-list", mcp("vat_eu_sales_list"), "Prepare EU sales evidence.", { dependsOn: ["vat-report"] }),
    write("record-filing-evidence", mcp("vat_filing_evidence_record"), "Record a reviewed B-field classification or statutory refund only when supported by evidence.", { dependsOn: ["vat-report|eu-sales-list"], condition: "Required only for non-zero B fields or refunds.", expectedIdempotent: true, retryClass: "natural-idempotent", canonicalRecords: ["VAT filing evidence events", "audit log"] }),
    read("tastselv-form", mcp("vat_filing"), "Read the exact whole-kroner TastSelv form; it never submits to Skattestyrelsen.", { dependsOn: ["vat-report|eu-sales-list|record-filing-evidence"], canonicalRecords: ["VAT filing form", "VAT report"] }),
  ], unsupportedBoundaries: ["This workflow does not file with an authority.", "Only documented taxable lines create input VAT."] }),
  workflow({ id: "exceptions-corrections", capabilityId: "exceptions-corrections", title: "Exceptions, corrections and reversals", intendedOutcome: "Resolve review blockers and correct posted entries through append-only compensating records.", steps: [
    read("list-exceptions", mcp("exceptions_list"), "Read unresolved exceptions."),
    write("resolve-exception", mcp("exception_resolve"), "Record the reviewed resolution.", { dependsOn: ["list-exceptions"], canonicalRecords: ["exception resolution audit"] }),
    read("review-journal", mcp("journal_list"), "Identify the exact entry requiring correction.", { dependsOn: ["resolve-exception"] }),
    write("reverse-journal", mcp("journal_reverse"), "Append a documented reversal.", { dependsOn: ["review-journal"], boundary: "irreversible", retryClass: "key-idempotent", uncertainOutcomeReadBack: mcp("journal_list"), canonicalRecords: ["reversal journal entry", "audit log"] }),
  ], alternatives: ["Use invoice_credit_note for an issued sales invoice."], unsupportedBoundaries: ["Posted entries and original documents are never overwritten or deleted."] }),
  workflow({ id: "period-close-reopen", capabilityId: "period-management", title: "Period readiness, close and reopen", intendedOutcome: "Inspect period readiness, close deliberately and reopen only through the supported correction path.", steps: [
    read("list-periods", mcp("period_list"), "Inspect period state and blockers."),
    read("close-readiness", mcp("period_close_readiness"), "Compute the exact read-only readiness packet and inspect every control.", { dependsOn: ["list-periods"], canonicalRecords: ["period close readiness packet"] }),
    write("review-readiness", mcp("period_close_review"), "Persist the reviewed packet before any close attempt.", { dependsOn: ["close-readiness"], boundary: "approval", canonicalRecords: ["period close review"] }),
    write("close-period", mcp("period_close"), "Close using the exact persisted review ID and packet hash; never retry after a stale packet.", { dependsOn: ["review-readiness"], boundary: "approval", canonicalRecords: ["period locks", "period close decision"] }),
    read("close-status", mcp("period_close_status"), "Poll the durable reviewed packet without recomputing readiness.", { dependsOn: ["review-readiness"] }),
    write("reopen-period", cli("period reopen"), "Reopen through the CLI-only audited operation.", { dependsOn: ["close-period"], condition: "Correction branch only.", boundary: "approval", requiresConfirmation: false, canonicalRecords: ["period reopen audit"] }),
    read("verify-period", mcp("period_list"), "Read back period state.", { dependsOn: ["close-period|reopen-period"] }),
  ], unsupportedBoundaries: ["Period reopen is CLI-only; no MCP parity is claimed."] }),
  workflow({ id: "backup-health-audit", capabilityId: "operations-assurance", title: "Backup, placement, health and audit verification", intendedOutcome: "Verify health and integrity, create and place a backup, and keep restore as an explicit destructive boundary.", steps: [
    read("healthcheck", mcp("system_healthcheck"), "Verify runtime and ledger readiness."),
    read("audit-verify", mcp("audit_verify"), "Verify the append-only hash chain.", { dependsOn: ["healthcheck"] }),
    read("backup-status", mcp("system_backup_status"), "Inspect backup currency and lock status.", { dependsOn: ["audit-verify"] }),
    write("create-backup", mcp("system_backup"), "Create the confirmed snapshot/archive.", { dependsOn: ["backup-status"], uncertainOutcomeReadBack: mcp("system_backup_status"), canonicalRecords: ["backup manifest", "backup audit"] }),
    write("place-backup", mcp("system_backup_place"), "Place the archive at a configured destination.", { dependsOn: ["create-backup"], canonicalRecords: ["backup placement evidence"] }),
    write("verify-placement", mcp("system_backup_verify_remote_placement"), "Verify placement against its checksum.", { dependsOn: ["place-backup"], canonicalRecords: ["verified placement evidence"] }),
    write("restore", mcp("system_restore_backup"), "Restore only to the explicitly confirmed target.", { dependsOn: ["create-backup"], condition: "Disaster-recovery branch only.", boundary: "destructive", expectedSafety: "destructive", retryClass: "unsafe-read-back", canonicalRecords: ["restored company root", "restore evidence"] }),
  ], unsupportedBoundaries: ["Rentemester does not choose provider retention policy.", "Restore never targets an implicit path."] }),
  workflow({ id: "group-intercompany", capabilityId: "group-intercompany", title: "Portfolio, group and intercompany overview", intendedOutcome: "Inspect a legal group, reconcile approved mappings and produce a read-only consolidated result.", steps: [
    read("portfolio-overview", mcp("portfolio_overview"), "Read accessible portfolio totals."),
    read("group-overview", cli("group overview"), "Read the effective-dated group graph.", { dependsOn: ["portfolio-overview"] }),
    read("intercompany-reconcile", cli("group reconcile"), "Reconcile approved intercompany mappings.", { dependsOn: ["group-overview"] }),
    read("consolidated-report", cli("group consolidated-report"), "Produce the traceable read-only consolidation view.", { dependsOn: ["intercompany-reconcile"], canonicalRecords: ["workspace group graph", "approved mappings", "approved eliminations", "derived consolidation view"] }),
  ], unsupportedBoundaries: ["Each legal entity keeps its own ledger.", "Group operations remain CLI/HTTP-only where no MCP operation is listed."] }),
  workflow({ id: "cfo-analytics", capabilityId: "cfo-analytics", title: "Source-linked CFO analytics", intendedOutcome: "Query current and archived accounting history with row-level source evidence, without changing any ledger.", steps: [
    read("query", mcp("cfo_analytics_query"), "Query the versioned company, juxtaposed portfolio, or approved group analysis.", { outputIdentities:["source journal/archive identifiers", "reconciliation evidence"], canonicalRecords:["posted journal lines", "import archive rows", "approved consolidated report"] }),
  ], unsupportedBoundaries:["Portfolio output is not legal consolidation and never applies eliminations or inferred FX conversion.","Group output fails closed unless an approved existing consolidation profile supports the requested scope.","No AI classification is added to accounting facts."] }),
  workflow({ id: "intercompany-disposition", capabilityId: "group-intercompany", title: "Intercompany disposition evidence lifecycle", intendedOutcome: "Create a source-linked two-sided disposition, approve it separately, and link each independently posted legal-ledger journal.", steps: [
    read("plan", mcp("intercompany_disposition_plan"), "Validate the exact evidence and both expected sides as a dry-run with no ledger effects.", { boundary:"dry-run", canonicalRecords:["validated disposition payload"] }),
    write("propose", mcp("intercompany_disposition_propose"), "Record the two-sided expected economic disposition and source evidence; it never posts either ledger.", { dependsOn:["plan"], canonicalRecords: ["intercompany disposition proposal", "party and corporate-record references"] }),
    write("approve", mcp("intercompany_disposition_approve"), "Approve the exact payload with a distinct stable principal.", { dependsOn:["propose"], boundary:"approval", canonicalRecords:["intercompany disposition approval"] }),
    write("link-left", mcp("intercompany_disposition_link"), "Validate and link the already posted left-company journal at its current ledger head.", { dependsOn:["approve"], boundary:"approval", canonicalRecords:["left legal-ledger link"] }),
    write("link-right", mcp("intercompany_disposition_link"), "Validate and link the independently posted right-company journal at its current ledger head.", { dependsOn:["approve"], boundary:"approval", canonicalRecords:["right legal-ledger link"] }),
    read("status", mcp("intercompany_disposition_status"), "Inspect one-sided, stale/reversed, overdue or fully linked status without mutation.", { dependsOn:["link-left|link-right"] }),
    write("settle", mcp("intercompany_disposition_settle"), "Append settlement evidence only after both current legal-ledger links validate.", { dependsOn:["status"], boundary:"approval", canonicalRecords:["settlement evidence"] }),
    write("reopen", mcp("intercompany_disposition_reopen"), "Reopen an ended lifecycle only after current journal and ledger-head validation.", { dependsOn:["settle"], condition:"Correction branch only.", boundary:"approval", canonicalRecords:["reopen decision"] }),
    write("supersede", mcp("intercompany_disposition_supersede"), "Append a replacement reference rather than deleting a mistaken active disposition.", { dependsOn:["propose"], condition:"Replacement branch only.", boundary:"approval", canonicalRecords:["supersession decision"] }),
  ], unsupportedBoundaries:["This flow never posts, reverses or eliminates either legal ledger.", "Group eliminations remain a separate read-only/reporting concern."] }),
  workflow({ id: "digisense-nemhandel", capabilityId: "digisense-nemhandel", title: "DigiSense and NemHandel onboarding, send, status and inbound", intendedOutcome: "Configure and onboard, send at most once, read status after uncertainty and ingest inbound documents with deduplication.", steps: [
    read("onboarding-status", mcp("efaktura_onboarding_status"), "Inspect environment and readiness."),
    write("configure", mcp("efaktura_konfigurer"), "Store provider configuration through the secret boundary.", { dependsOn: ["onboarding-status"], expectedIdempotent: true, retryClass: "external-provider-reconciled", canonicalRecords: ["e-invoice configuration audit"] }),
    write("onboard", mcp("efaktura_onboard"), "Register in the selected environment.", { dependsOn: ["configure"], expectedIdempotent: true, retryClass: "external-provider-reconciled", uncertainOutcomeReadBack: mcp("efaktura_onboarding_status"), canonicalRecords: ["participant registration evidence"] }),
    write("send", mcp("efaktura_send"), "Submit the issued invoice once.", { dependsOn: ["onboard"], expectedIdempotent: true, retryClass: "external-provider-reconciled", outputIdentities: ["submissionId"], uncertainOutcomeReadBack: mcp("efaktura_status"), canonicalRecords: ["Peppol submission events"] }),
    write("delivery-status", mcp("efaktura_status"), "Perform the actor-audited, confirmed status lookup for the existing submission.", { dependsOn: ["send"], expectedIdempotent: true, retryClass: "external-provider-reconciled", inputIdentities: ["submissionId"] }),
    write("receive", mcp("efaktura_modtag"), "Poll and ingest inbound documents with deduplication.", { dependsOn: ["onboard"], condition: "Inbound branch.", expectedIdempotent: true, retryClass: "external-provider-reconciled", canonicalRecords: ["inbound documents", "deduplication evidence"] }),
  ], unsupportedBoundaries: ["Test and production are explicit.", "Discovery exposes no credentials or participant identities."] }),
  workflow({ id: "imports-dinero", capabilityId: "imports", title: "Imports including Dinero", intendedOutcome: "Validate a source export, dry-run the supported import and apply only the explicit cut-over scope.", steps: [
    read("supported-systems", cli("import systems"), "Discover supported systems and required files."),
    read("dry-run", cli("import run"), "Validate/dry-run the selected source and fiscal scope.", { dependsOn: ["supported-systems"], boundary: "dry-run", requiresActor: true, requiredArguments: ["--dry-run"], outputIdentities: ["source hashes", "import plan"] }),
    write("apply-import", cli("import run"), "Apply the exact validated import.", { dependsOn: ["dry-run"], requiresConfirmation: false, requiredArguments: ["--apply"], outputIdentities: ["import run identity"], canonicalRecords: ["imported ledger records", "source-hash evidence", "import audit"] }),
    read("plan-legacy-receivables", mcp("invoice_imported_receivables_backfill_plan"), "For an already accepted pre-v36 Dinero cut-over only: bind a separately hash-verified schedule to the immutable source, explicit control date, debtors balance, ledger head and audit head without replaying the import.", { dependsOn:["dry-run"], condition:"Legacy accepted import has no canonical imported-receivable schedule.", boundary:"dry-run", inputIdentities:["dineroImportAttemptId","sourceRawSha256","canonicalInventorySha256","artifactSha256","controlDate","controlAccountNo"], outputIdentities:["planHash","scheduleHash"], canonicalRecords:["legacy imported receivable backfill plan"] }),
    write("apply-legacy-receivables", mcp("invoice_imported_receivables_backfill_apply"), "Append only the exact reviewed legacy schedule and audit evidence; never replay or rewrite import state.", { dependsOn:["plan-legacy-receivables"], condition:"Legacy branch only.", boundary:"approval", expectedIdempotent:true, retryClass:"natural-idempotent", inputIdentities:["planHash","idempotencyKey"], uncertainOutcomeReadBack:mcp("invoice_imported_receivables"), canonicalRecords:["imported receivable headers","imported receivable events","imported receivable boundary","legacy backfill audit"] }),
    read("plan-imported-receivable-settlement", mcp("invoice_imported_receivable_settlement_plan"), "Bind one canonical imported DKK receivable identity and one immutable, unreconciled bank receipt. The plan makes no changes.", { dependsOn:["imported-receivables"], boundary:"dry-run", inputIdentities:["scheduleHash","externalInvoiceId","bankTransactionId"], outputIdentities:["planHash","bankTransactionHash"], canonicalRecords:["imported receivable header","bank transaction"] }),
    write("apply-imported-receivable-settlement", mcp("invoice_imported_receivable_settlement_apply"), "Atomically post the bank/control journal, reconcile the selected receipt and append payment evidence for the exact reviewed plan.", { dependsOn:["plan-imported-receivable-settlement"], boundary:"approval", expectedIdempotent:true, retryClass:"natural-idempotent", inputIdentities:["planHash","idempotencyKey"], uncertainOutcomeReadBack:mcp("invoice_imported_receivable_settlement_status"), canonicalRecords:["journal entry","bank reconciliation","imported receivable event","settlement audit"] }),
    read("imported-receivable-settlement-status", mcp("invoice_imported_receivable_settlement_status"), "Read back the immutable settlement by bank transaction after an uncertain outcome.", { dependsOn:["apply-imported-receivable-settlement"], inputIdentities:["bankTransactionId"], canonicalRecords:["imported receivable bank settlement"] }),
    write("import-contacts", cli("import contacts"), "Import Dinero contacts idempotently.", { dependsOn: ["supported-systems"], condition: "Optional contacts branch.", expectedIdempotent: true, requiresConfirmation: false, retryClass: "natural-idempotent", canonicalRecords: ["customers", "vendors", "contact import audit"] }),
    read("archive", mcp("import_archive_list"), "Read the retained source archive.", { dependsOn: ["apply-import"] }),
    read("imported-receivables", mcp("invoice_imported_receivables"), "Read source-evidenced opening debtors at an explicit cutoff. This is a separate archive schedule, never a native invoice list.", { dependsOn: ["apply-import"], inputIdentities: ["asOf"], canonicalRecords: ["imported receivable headers", "imported receivable events", "source hashes"] }),
  ], unsupportedBoundaries: ["No company-specific mapping is inferred.", "The legacy backfill never replays journals, documents, archive years, paths, import attempts or bank reconciliations.", "A changed ledger/audit head, source hash, schedule or control balance invalidates the reviewed plan."] }),
  workflow({ id: "privacy-governance", capabilityId: "privacy", title: "GDPR discovery and export", intendedOutcome: "Discover and export data-subject records through audited, confirmed operations.", steps: [
    write("discover", mcp("gdpr_discover"), "Create audited discovery evidence.", { canonicalRecords: ["GDPR audit events"] }),
    write("export", mcp("gdpr_export"), "Create the confirmed subject export.", { dependsOn: ["discover"], canonicalRecords: ["GDPR export audit"] }),
    read("audit", mcp("gdpr_audit_log"), "Read back privacy evidence.", { dependsOn: ["export"] }),
  ], unsupportedBoundaries: ["Erasure/forget uses dedicated CLI contracts."] }),
  workflow({ id: "asset-register-depreciate", capabilityId: "fixed-assets", title: "Fixed asset lifecycle", intendedOutcome: "Register an asset and post the next supported depreciation period.", steps: [
    write("register", mcp("asset_register"), "Register the asset and schedule.", { canonicalRecords: ["assets", "depreciation schedule"] }),
    write("depreciate", mcp("asset_depreciate"), "Post the next reviewed depreciation.", { dependsOn: ["register"], boundary: "irreversible", uncertainOutcomeReadBack: mcp("asset_register_report"), canonicalRecords: ["asset depreciation events", "journal entries"] }),
    read("report", mcp("asset_register_report"), "Read back the asset register.", { dependsOn: ["depreciate"] }),
  ] }),
  workflow({ id: "mileage-register-report", capabilityId: "mileage", title: "Mileage registration and reporting", intendedOutcome: "Register documented business mileage and produce the supported report.", steps: [
    write("log", mcp("mileage_log"), "Register one documented trip.", { canonicalRecords: ["mileage log"] }),
    read("list", mcp("mileage_list"), "Read registered trips.", { dependsOn: ["log"] }),
    read("report", mcp("mileage_report"), "Build the mileage report.", { dependsOn: ["list"] }),
  ] }),
  workflow({ id: "planning-accrual-reporting", capabilityId: "planning-reporting", title: "Budget, accrual and tax preparation", intendedOutcome: "Maintain budget/accrual records and prepare reports without external filing.", steps: [
    write("set-budget", mcp("budget_set"), "Set reviewed budget values.", { canonicalRecords: ["budgets"] }),
    read("budget-report", mcp("budget_vs_actual"), "Compare budget with ledger actuals.", { dependsOn: ["set-budget"] }),
    write("register-accrual", mcp("accrual_register"), "Register an accrual schedule.", { canonicalRecords: ["accrual schedules"] }),
    read("tax-prepare", mcp("tax_return_prepare"), "Prepare tax-return material.", { dependsOn: ["budget-report", "register-accrual"], boundary: "review" }),
  ], unsupportedBoundaries: ["Tax/report material is not filed and is not tax advice."] }),
  workflow({ id: "accounting-dimensions", capabilityId: "accounting-dimensions", title: "Reviewed accounting dimensions", intendedOutcome: "Classify immutable journal lines by approved company dimensions without changing legal amounts, VAT or journal hashes.", steps: [
    write("define", mcp("dimension_definition_create"), "Define a company-scoped dimension.", { canonicalRecords:["dimension definition events"] }),
    write("member", mcp("dimension_member_create"), "Define an active or historical member.", { dependsOn:["define"], canonicalRecords:["dimension member events"] }),
    write("lifecycle", mcp("dimension_member_lifecycle"), "Append a reviewed member lifecycle event without losing its stable identity or historical label.", { dependsOn:["member"], canonicalRecords:["dimension member events"] }),
    read("plan", mcp("dimension_assignment_plan"), "Produce the exact read-only allocation plan and hash.", { dependsOn:["member"], boundary:"dry-run", canonicalRecords:["dimension assignment plan"] }),
    write("apply", mcp("dimension_assignment_apply"), "Append the reviewed hash-bound allocation.", { dependsOn:["plan"], canonicalRecords:["dimension assignment events"] }),
    read("inspect", mcp("dimension_assignment_list"), "Read source-linked assignment history and drilldown ids.", { dependsOn:["apply"] }),
    write("replace", mcp("dimension_assignment_replace"), "Atomically supersede the expected assignment and append the exact reviewed replacement.", { dependsOn:["inspect"], boundary:"review", canonicalRecords:["dimension assignment supersession","dimension assignment events"] }),
    write("supersede", mcp("dimension_assignment_supersede"), "Retire a classification append-only without changing the journal.", { dependsOn:["inspect"], boundary:"review", canonicalRecords:["dimension assignment supersession"] }),
    read("budget-plan", mcp("dimension_budget_plan"), "Produce an exact allocation plan that reconciles to the reviewed account budget.", { dependsOn:["member"], boundary:"dry-run", canonicalRecords:["dimension budget plan"] }),
    write("budget-apply", mcp("dimension_budget_apply"), "Append the complete reviewed, hash-bound allocation set.", { dependsOn:["budget-plan"], boundary:"review", expectedIdempotent:true, retryClass:"natural-idempotent", canonicalRecords:["dimension budget events"] }),
    read("budget-inspect", mcp("dimension_budget_list"), "Read the current reviewed allocation set.", { dependsOn:["budget-apply"] }),
  ], unsupportedBoundaries:["Dimensions never change account, VAT, currency, legal entity, legal totals or journal hashes.","Imported dimensions remain provenance until explicitly reviewed."] }),
  workflow({ id: "posting-rule-review", capabilityId: "posting-rules", title: "Company-specific posting rule review", intendedOutcome: "Propose, independently approve and explain a reusable audited posting rule.", steps: [
    write("propose", mcp("posting_rule_propose"), "Propose an inert rule.", { expectedIdempotent: true, retryClass: "natural-idempotent", canonicalRecords: ["posting rule proposal"] }),
    write("approve", mcp("posting_rule_approve"), "Approve with reviewer separation.", { dependsOn: ["propose"], boundary: "approval", canonicalRecords: ["approved posting rule"] }),
    read("explain", mcp("posting_rule_explain"), "Explain the active rule and evidence.", { dependsOn: ["approve"] }),
  ] }),
  workflow({ id: "workspace-party-lifecycle", capabilityId: "workspace-parties", title: "Workspace party lifecycle", intendedOutcome: "Create a canonical party, attach only company-scoped roles, and review an explicit duplicate proposal without automatic identity merging.", steps: [
    read("search", mcp("workspace_party_search"), "Search only parties visible through the selected company."),
    write("create", mcp("workspace_party_create"), "Create source-backed identity evidence without a ledger effect.", { dependsOn:["search"], canonicalRecords:["workspace party events", "party identifier assertions"] }),
    write("link-role", mcp("workspace_party_link_role"), "Attach a role and defaults only for the selected company.", { dependsOn:["create"], canonicalRecords:["company party role"] }),
    write("propose-merge", mcp("workspace_party_propose_merge"), "Record an explicit human-reviewed duplicate proposal.", { dependsOn:["link-role"], boundary:"review", canonicalRecords:["party merge proposal"] }),
    write("approve-merge", mcp("workspace_party_approve_merge"), "Approve the exact proposal and append a supersession event.", { dependsOn:["propose-merge"], boundary:"approval", canonicalRecords:["party merge approval", "party supersession"] }),
    read("inspect", mcp("workspace_party_inspect"), "Read the visible canonical history and local roles.", { dependsOn:["link-role|approve-merge"] }),
  ], unsupportedBoundaries:["Name, amount or alias similarity never auto-merges a legal identity.", "Company-local defaults never become workspace posting rules."] }),
  workflow({ id: "document-party-resolution", capabilityId: "document-party-resolution", title: "Document party resolution", intendedOutcome: "Make exactly one visible party-resolution state without changing document evidence, VAT, or journals.", steps: [
    read("list", mcp("documents_party_link_list"), "List resolved, internal-no-external-party, and unresolved documents."),
    read("plan", mcp("documents_party_link_plan"), "Plan an exact evidence-bound canonical party relation."),
    write("apply", mcp("documents_party_link_apply"), "Append the confirmed canonical party relation.", { dependsOn:["plan"], expectedIdempotent:true, retryClass:"natural-idempotent", canonicalRecords:["document party link event"] }),
    write("no-external-party", mcp("documents_internal_no_external_party"), "Confirm an internal voucher intentionally has no external party.", { expectedIdempotent:true, retryClass:"natural-idempotent", canonicalRecords:["document party resolution event"] }),
    write("supersede", mcp("documents_internal_no_external_party_supersede"), "Append a correction to the exact no-party decision.", { boundary:"review", expectedIdempotent:true, retryClass:"natural-idempotent", canonicalRecords:["document party resolution supersession"] }),
  ], unsupportedBoundaries:["Party resolution never changes linked/posted evidence bytes, VAT, or journals.", "Names alone never resolve a canonical party."] }),
  workflow({ id: "corporate-record-lifecycle", capabilityId: "corporate-records", title: "Corporate record lifecycle", intendedOutcome: "Store immutable governance evidence, link it to permitted scope, enrich it append-only and supersede rather than overwrite it.", steps: [
    write("ingest", mcp("corporate_record_ingest"), "Ingest bytes and immutable SHA-256 evidence without ledger, group or filing side effects.", { canonicalRecords:["corporate record original bytes", "corporate record ingest event"] }),
    write("link", mcp("corporate_record_link"), "Attach a typed scope link without changing bytes.", { dependsOn:["ingest"], canonicalRecords:["corporate record scope assertion"] }),
    write("enrich", mcp("corporate_record_enrich"), "Append reviewed metadata/provenance.", { dependsOn:["link"], canonicalRecords:["corporate record enrichment event"] }),
    write("supersede", mcp("corporate_record_supersede"), "Append a correction chain to a replacement record.", { dependsOn:["enrich"], canonicalRecords:["corporate record supersession"] }),
    read("inspect", mcp("corporate_record_inspect"), "Read visible metadata/history."),
    read("download", mcp("corporate_record_download"), "Read verified original bytes only after scope authorization.", { dependsOn:["inspect"] }),
  ], unsupportedBoundaries:["Corporate records are governance evidence, never accounting vouchers or filing actions.", "Original bytes and hashes are never overwritten or deleted."] }),
  workflow({ id:"company-knowledge-lifecycle", capabilityId:"company-knowledge", title:"Company operating knowledge", intendedOutcome:"Retrieve or maintain source-backed, effective-dated operating context without changing canonical accounting settings.", steps:[
    read("context",mcp("company_knowledge_context"),"Read compact machine-readable context as of a declared date."),
    write("propose",mcp("company_knowledge_propose"),"Propose one typed, sourced assertion without a ledger effect.",{canonicalRecords:["company knowledge assertion"]}),
    write("review",mcp("company_knowledge_review"),"Approve or reject exactly one assertion append-only.",{dependsOn:["propose"],boundary:"approval",canonicalRecords:["company knowledge review event"]}),
    write("supersede",mcp("company_knowledge_supersede"),"Replace approved context through a new assertion and supersession link.",{dependsOn:["review"],canonicalRecords:["company knowledge supersession"]}),
  ],unsupportedBoundaries:["Knowledge never changes ledger, VAT, legal classification or group membership automatically.","Conflicting approved singleton facts remain conflicts; no latest-write selection occurs."]}),
  workflow({ id:"ownership-graph-review", capabilityId:"ownership-graph", title:"Reviewable ownership and control graph", intendedOutcome:"Discover an as-of legal ownership graph, record a source-hashed registry proposal, review it, then apply only the exact approved diff.", steps:[
    read("query",mcp("ownership_graph_query"),"Read visible approved facts as of a declared date; it is not a consolidation."),
    read("history",mcp("ownership_snapshot_history"),"Read only snapshot history whose every legal endpoint remains authorized."),
    write("propose",mcp("ownership_snapshot_propose"),"Store a deterministic source snapshot and inert diff.",{canonicalRecords:["ownership source snapshot","ownership proposal diff"]}),
    write("review",mcp("ownership_snapshot_review"),"Approve or reject exactly one source snapshot.",{dependsOn:["propose"],boundary:"approval",canonicalRecords:["ownership snapshot review event"]}),
    write("apply",mcp("ownership_snapshot_apply"),"Apply exact approved hashes append-only with live scoped authority.",{dependsOn:["review"],boundary:"irreversible",canonicalRecords:["approved ownership facts","ownership apply event"]}),
  ],unsupportedBoundaries:["Registry observations never automatically overwrite, end or legally conclude ownership.","The v1 group manifest remains authoritative; minority, interval, hidden or incomplete ownership is never inferred as consolidation."]}),
];

type CapabilityTuple = [string, string, string, string, string[], string[], AgentScope, string[]];
const capabilityTuples: CapabilityTuple[] = [
  ["company-workspace", "Company and workspace setup", "Set up and discover companies without leaking inaccessible state.", "company", ["create company", "switch company", "discover workspace", "bootstrap local service credential", "rotate service credential"], ["setup", "workspace", "company profile", "local service principal", "credential rotation"], "workspace", ["company-workspace-setup", "local-service-principal-lifecycle"]],
  ["company-knowledge", "Company operating knowledge", "Retrieve and review source-backed, dated company operating facts.", "company", ["company context", "operating profile", "company knowledge"], ["knowledge", "products", "revenue model", "market"], "company", ["company-knowledge-lifecycle"]],
  ["document-intake", "Document and mail intake", "Store source documents and mail attachments for review.", "documents", ["ingest document", "mail intake", "review invoice extraction", "review incomplete purchase evidence", "review non-EU reverse charge evidence", "record external payroll evidence"], ["bilag", "imap", "attachment", "incomplete invoice", "non-EU reverse charge", "payroll evidence"], "company", ["document-mail-intake", "incomplete-purchase-evidence-review", "non-eu-reverse-charge-evidence-review", "external-payroll-evidence-journal"]],
  ["non-cash-balance-corrections", "Non-cash balance corrections", "Record one hash-bound DKK correction between eligible balance accounts, including documented legacy opening creditor reclassification.", "ledger", ["correct balance without bank movement", "non-cash balance correction", "legacy opening creditor", "internal balance voucher"], ["balance correction", "internal voucher", "no bank movement", "DKK", "primobalance", "creditor"], "company", ["non-cash-balance-correction","legacy-opening-creditor-reclassification"]],
  ["workspace-document-inbox", "Workspace document inbox", "Route immutable incoming evidence to one authorized legal entity without a workspace ledger.", "documents", ["route incoming document", "assign workspace inbox", "review ambiguous company"], ["workspace inbox", "routing", "recipient alias", "buyer VAT"], "workspace", ["workspace-document-inbox"]],
  ["bank-bookkeeping", "Bank reconciliation and bookkeeping batch", "Import activity, review one canonical bank work queue and apply a hash-bound batch.", "bank", ["reconcile bank", "match bank transactions", "bookkeeping workbench", "bookkeeping batch", "correct bank reconciliation"], ["bank import", "workbench", "dry run", "plan hash", "reconciliation correction"], "company", ["bank-reconciliation-batch", "bookkeeping-workbench", "bank-reconciliation-correction"]],
  ["supplier-purchases", "Supplier expenses and payables", "Book supplier invoices directly or through payable handling.", "purchases", ["book supplier invoice", "pay supplier invoice", "book expense", "adopt legacy payable", "provisional purchase case", "group purchase evidence"], ["vendor", "payable", "purchase VAT", "legacy backfill", "purchase case", "purchase overview"], "company", ["supplier-expense-booking", "supplier-payable-handling", "purchase-case-lifecycle", "purchase-case-group-review", "direct-bank-purchase-payable-correction", "legacy-bank-payable-backfill"]],
  ["supplier-commitments", "Supplier commitments and 13-week liquidity", "Review recurring supplier commitments and inspect a source-linked cash forecast without generating payments.", "planning", ["track supplier subscription", "forecast cash 13 weeks", "review renewals"], ["commitment", "subscription", "cash forecast", "renewal"], "company", ["supplier-commitment-forecast"]],
  ["customer-invoicing", "Customer invoice lifecycle", "Create customers and handle issue, delivery, payment, reminder and correction.", "sales", ["issue customer invoice", "send invoice", "record payment", "send reminder", "credit note"], ["customer", "invoice", "settlement"], "company", ["customer-invoice-lifecycle"]],
  ["vat", "VAT preparation", "Validate and post supported VAT treatments and prepare evidence.", "vat", ["prepare VAT", "domestic purchase VAT", "reverse charge"], ["moms", "VIES", "input VAT"], "company", ["vat-preparation"]],
  ["exceptions-corrections", "Exceptions and corrections", "Resolve blockers and correct through append-only reversals.", "ledger", ["resolve exception", "reverse posting", "correct bookkeeping"], ["correction", "credit note", "audit"], "company", ["exceptions-corrections"]],
  ["period-management", "Period management", "Inspect, close and explicitly reopen periods.", "period", ["close period", "reopen period", "period readiness"], ["lock", "fiscal period"], "company", ["period-close-reopen"]],
  ["operations-assurance", "Backup, health and audit", "Verify integrity and create, place, verify or restore backups.", "system", ["verify backup", "healthcheck", "verify audit", "restore backup"], ["readiness", "checksum", "placement"], "system", ["backup-health-audit"]],
  ["group-intercompany", "Portfolio, group and intercompany", "Inspect group state, reconcile mappings and document two-sided dispositions without cross-ledger posting.", "group", ["group overview", "intercompany reconciliation", "intercompany disposition"], ["portfolio", "elimination", "legal group", "intercompany disposition"], "legal-group", ["group-intercompany", "intercompany-disposition"]],
  ["cfo-analytics", "CFO analytics", "Query source-linked current and archived history across an authorised company, portfolio or approved group report.", "reporting", ["supplier spend", "customer revenue", "historical postings", "source drilldown"], ["CFO", "analytics", "historical", "supplier", "customer", "archive"], "workspace", ["cfo-analytics"]],
  ["digisense-nemhandel", "DigiSense and NemHandel", "Onboard, send once, read status and receive electronic invoices.", "efaktura", ["send e-invoice", "NemHandel onboarding", "receive e-invoice"], ["Digisense", "Peppol", "OIOUBL"], "company", ["digisense-nemhandel"]],
  ["imports", "Imports including Dinero", "Validate and apply supported cut-over imports.", "imports", ["import from Dinero", "migrate accounting data", "import contacts"], ["archive", "cut-over", "source hash"], "company", ["imports-dinero"]],
  ["privacy", "Privacy governance", "Perform audited GDPR discovery and export.", "privacy", ["GDPR export", "data subject discovery"], ["privacy", "erasure"], "company", ["privacy-governance"]],
  ["fixed-assets", "Fixed assets", "Register assets and post depreciation.", "assets", ["register asset", "depreciate asset"], ["anlæg", "write-off"], "company", ["asset-register-depreciate"]],
  ["mileage", "Mileage", "Register and report documented business mileage.", "mileage", ["log mileage", "mileage report"], ["trip", "kilometres"], "company", ["mileage-register-report"]],
  ["planning-reporting", "Planning and reporting", "Maintain budgets/accruals and prepare tax/reporting material.", "reporting", ["budget versus actual", "register accrual", "prepare tax return", "annual report"], ["forecast", "report", "tax"], "company", ["planning-accrual-reporting"]],
  ["accounting-dimensions", "Accounting dimensions", "Classify source-linked journal lines by reviewed projects, products, departments or cost centres without changing legal accounting.", "reporting", ["classify project cost", "assign cost centre", "dimension actuals"], ["dimension", "project", "cost centre", "department", "allocation"], "company", ["accounting-dimensions"]],
  ["posting-rules", "Posting rules", "Propose, approve and explain reusable posting rules.", "rules", ["create posting rule", "approve bookkeeping rule"], ["automation", "review separation"], "company", ["posting-rule-review"]],
  ["workspace-parties", "Workspace parties", "Maintain canonical counterparties with isolated company roles and reviewed supersession.", "master data", ["create canonical party", "link company party role", "review duplicate party"], ["party", "counterparty", "identity", "vendor role"], "workspace", ["workspace-party-lifecycle"]],
  ["document-party-resolution", "Document party resolution", "Resolve a document to canonical party relations, an explicit internal no-party decision, or a bounded unresolved state.", "documents", ["link document party", "confirm internal voucher has no external party", "inspect document party resolution"], ["document party", "issuer", "supplier", "payer", "payment descriptor"], "company", ["document-party-resolution"]],
  ["corporate-records", "Corporate records", "Store immutable corporate and governance evidence with typed, access-controlled links.", "governance", ["ingest corporate record", "link governance evidence", "supersede corporate record"], ["corporate record", "governance", "articles", "ownership evidence"], "workspace", ["corporate-record-lifecycle"]],
  ["ownership-graph", "Ownership and control graph", "Review source-backed, party-aware ownership and control facts without changing legal ledgers or inferring consolidation.", "governance", ["review ownership", "record registry ownership", "query control graph"], ["ownership", "shareholder", "control", "registry diff"], "legal-group", ["ownership-graph-review"]],
];

export const AGENT_CAPABILITIES: readonly AgentCapability[] = capabilityTuples.map(([id, title, purpose, domain, outcomes, keywords, scope, workflowIds]) => ({
  id, title, purpose, domain, outcomes, keywords, scope, supportStatus: "supported", maturity: "stable", workflowIds,
  canonicalState: AGENT_WORKFLOWS.filter((item) => workflowIds.includes(item.id)).flatMap((item) => item.steps.flatMap((itemStep) => itemStep.canonicalRecords)),
  unsupportedBoundaries: AGENT_WORKFLOWS.filter((item) => workflowIds.includes(item.id)).flatMap((item) => item.unsupportedBoundaries),
}));

function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") { const record = value as Record<string, unknown>; return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`).join(",")}}`; }
  return JSON.stringify(value);
}

export const AGENT_CATALOGUE_HASH = createHash("sha256").update(canonicalJson({ schemaVersion: AGENT_CATALOGUE_SCHEMA_VERSION, capabilities: AGENT_CAPABILITIES, workflows: AGENT_WORKFLOWS })).digest("hex");

export function catalogueIdentity() {
  let ruleBundleVersion: string | null = null;
  try { ruleBundleVersion = currentRuleBundleVersion(); } catch {}
  return { schemaVersion: AGENT_CATALOGUE_SCHEMA_VERSION, hash: AGENT_CATALOGUE_HASH, entryPoint: AGENT_CATALOGUE_ENTRY_POINT, capabilityCount: AGENT_CAPABILITIES.length, workflowCount: AGENT_WORKFLOWS.length, coverage: coverageIdentity(), build: getBuildIdentity(), provenance: getReleaseProvenance(), ruleBundleVersion };
}

export type LiveTool = { name: string; annotations?: { readOnlyHint?: boolean; destructiveHint?: boolean; idempotentHint?: boolean } };
export type LiveOperationSources = { tools?: readonly LiveTool[]; commands?: readonly DiscoveryCommand[]; routes?: readonly DiscoveryRoute[]; unavailableSurfaces?: Array<"mcp" | "cli" | "http"> };

export function operationId(reference: OperationReference): string { return reference.surface === "mcp" ? `mcp:${reference.name}` : reference.surface === "cli" ? `cli:${reference.key}` : `http:${reference.method} ${reference.pattern}`; }

function resolveOperation(reference: OperationReference, sources: LiveOperationSources) {
  if (reference.surface === "mcp") {
    if (sources.unavailableSurfaces?.includes("mcp")) return { ...reference, id: operationId(reference), resolved: null, reason: "MCP registry is not part of this HTTP transport; resolve with MCP tools/list." };
    const tool = sources.tools?.find((candidate) => candidate.name === reference.name);
    if (!tool) return { ...reference, id: operationId(reference), resolved: false, reason: "Live MCP tool is not registered." };
    if (!tool.annotations || typeof tool.annotations.readOnlyHint !== "boolean") return { ...reference, id: operationId(reference), resolved: false, reason: "Live MCP tool has no safety annotations." };
    return { ...reference, id: operationId(reference), resolved: true, safety: tool.annotations.readOnlyHint ? "read" : tool.annotations.destructiveHint ? "destructive" : "write", idempotent: tool.annotations.idempotentHint === true };
  }
  if (reference.surface === "cli") return sources.commands?.some((candidate) => candidate.key === reference.key) ? { ...reference, id: operationId(reference), resolved: true } : { ...reference, id: operationId(reference), resolved: false, reason: "Canonical CLI command is not registered." };
  const route = sources.routes?.find((candidate) => candidate.method === reference.method && candidate.pattern === reference.pattern);
  return route ? { ...reference, id: operationId(reference), resolved: true, safety: route.effect === "read" ? "read" : route.effect === "destructive" ? "destructive" : "write" } : { ...reference, id: operationId(reference), resolved: false, reason: "HTTP route is not catalogued." };
}

export function searchCapabilities(query: string | undefined, cursor: number, limit: number, sources?: LiveOperationSources) {
  const tokens = (query ?? "").trim().toLowerCase().split(/\s+/).filter(Boolean);
  const matching = AGENT_CAPABILITIES.filter((item) => { const haystack = [item.id, item.title, item.purpose, item.domain, ...item.outcomes, ...item.keywords].join(" ").toLowerCase(); return tokens.every((token) => haystack.includes(token)); });
  const bindings = sources ? discoverableOperationBindings(sources) : [];
  const items = matching.slice(cursor, cursor + limit).map(({ canonicalState: _canonicalState, keywords: _keywords, ...item }) => ({
    ...item,
    operations: bindings.filter((binding) => binding.capabilityIds.includes(item.id)),
  }));
  return { catalogue: catalogueIdentity(), total: matching.length, count: items.length, cursor, limit, hasMore: cursor + items.length < matching.length, nextCursor: cursor + items.length < matching.length ? cursor + items.length : null, items };
}

export function describeWorkflow(id: string, sources: LiveOperationSources) {
  const item = AGENT_WORKFLOWS.find((candidate) => candidate.id === id);
  if (!item) return null;
  const capability = AGENT_CAPABILITIES.find((candidate) => candidate.id === item.capabilityId)!;
  const steps = item.steps.map((itemStep) => ({ ...itemStep, operation: resolveOperation(itemStep.operation, sources), uncertainOutcomeReadBack: itemStep.uncertainOutcomeReadBack ? resolveOperation(itemStep.uncertainOutcomeReadBack, sources) : undefined }));
  const unresolved = steps.filter((itemStep) => itemStep.operation.resolved === false).map((itemStep) => itemStep.operation.id);
  return { catalogue: catalogueIdentity(), capability, workflow: { ...item, steps, live: unresolved.length === 0, unresolvedOperations: unresolved } };
}

export const AGENT_DISCOVERY_INTERNALS = { canonicalJson, resolveOperation };

export type DiscoveryCommand = {
  key: string;
  mutating?: boolean;
  sideEffecting?: boolean;
  allowedFlags?: readonly string[];
};
export type DiscoveryRoute = {
  method: string;
  pattern: string;
  effect?: string;
};
export type DiscoveryOperationBinding = {
  id: string;
  capabilityIds: string[];
  safety: OperationSafety;
  idempotent: boolean | null;
  requiresActor: boolean;
  requiresConfirmation: boolean;
  retryClass: RetryClass;
};

/** These are the only MCP mutations with a durable caller-key receipt. Keep
 * this list deliberately small: adding an entry is a transaction/audit change,
 * not a metadata-only promise. */
export const KEY_IDEMPOTENT_MCP_OPERATIONS = RETRY_OPERATION_NAMES.keyIdempotent;
/** Explicitly reviewed domain-deduplication contracts.  Do not derive this
 * class from `idempotentHint`: that hint is evidence which this list validates,
 * not a substitute for a retry contract. */
const NATURAL_IDEMPOTENT_MCP_OPERATIONS = RETRY_OPERATION_NAMES.naturalIdempotent;

/** Provider calls may have an accepted remote identity. They must be reconciled
 * with that identity/status before a retry, even where the local action itself
 * has de-duplication. */
const EXTERNAL_PROVIDER_MCP_OPERATIONS = RETRY_OPERATION_NAMES.externalProviderReconciled;
const NATURAL_IDEMPOTENT_CLI_OPERATIONS = RETRY_OPERATION_NAMES.naturalIdempotentCli;

export function retryClassForOperation(id: string, source: { safety: OperationSafety; idempotent: boolean | null; external?: boolean }): RetryClass {
  if (source.safety === "read") return "safe-read";
  if (source.external || (id.startsWith("mcp:") && EXTERNAL_PROVIDER_MCP_OPERATIONS.has(id.slice(4)))) return "external-provider-reconciled";
  if (id.startsWith("mcp:") && KEY_IDEMPOTENT_MCP_OPERATIONS.has(id.slice(4))) return "key-idempotent";
  if ((id.startsWith("mcp:") && NATURAL_IDEMPOTENT_MCP_OPERATIONS.has(id.slice(4))) || (id.startsWith("cli:") && NATURAL_IDEMPOTENT_CLI_OPERATIONS.has(id.slice(4)))) return "natural-idempotent";
  return "unsafe-read-back";
}

type SurfaceName = "mcp" | "cli" | "http";
type SurfaceBaseline = { count: number; hash: string };

/**
 * Reviewed identities of the three public operation registries. The live
 * registries remain authoritative; these compact snapshots make additions and
 * removals require an explicit discovery review without copying hundreds of
 * operation names into a second hand-maintained catalogue.
 */
export const AGENT_SURFACE_BASELINES: Record<SurfaceName, SurfaceBaseline> = {
  // Public surface changes require an explicit discovery review.
  mcp: { count: 233, hash: "52d90690362bab5cd9ca70980348aa5464189c50df26c4298ad07acd35f2b303" },
  cli: { count: 285, hash: "e7b306c3b359c8a56be778d8dff7b6f436d9ed19cd5c9f67100329344f2b34f4" },
  http: { count: 223, hash: "2e524b3ef9843ccc36aff8fa8bf5431c1969523889ec674923491fbc8913c40b" },
};

const CAPABILITY_RULES: ReadonlyArray<{ capabilityId: string; pattern: RegExp }> = [
  { capabilityId: "non-cash-balance-corrections", pattern: /(?:documents_(?:ingest|list)|journal_(?:dry_run|post|list))/ },
  { capabilityId: "accounting-dimensions", pattern: /dimensions?|dimension[_-]/ },
  { capabilityId: "cfo-analytics", pattern: /(?:cfo[_-]analytics|report analytics|cfo-analytics)/ },
  { capabilityId: "workspace-document-inbox", pattern: /workspace[_-]inbox/ },
  { capabilityId: "corporate-records", pattern: /(?:corporate[_-]record|corporate-record)/ },
  { capabilityId: "workspace-parties", pattern: /(?:workspace[_-]party|^cli:party )/ },
  { capabilityId: "document-party-resolution", pattern: /documents?_party|party-link|internal-no-external-party/ },
  { capabilityId: "digisense-nemhandel", pattern: /(?:efaktura|digisense|peppol|send-public)/ },
  { capabilityId: "group-intercompany", pattern: /(?:group|portfolio)/ },
  { capabilityId: "posting-rules", pattern: /(?:posting[_-]rules?|posting_rule|agent-suggestions)/ },
  { capabilityId: "fixed-assets", pattern: /(?:asset|fixed-assets)/ },
  { capabilityId: "mileage", pattern: /mileage/ },
  { capabilityId: "privacy", pattern: /gdpr/ },
  { capabilityId: "period-management", pattern: /(?:period|fiscal-years)/ },
  { capabilityId: "vat", pattern: /(?:vat|moms|oss-report|eu-sales)/ },
  { capabilityId: "bank-bookkeeping", pattern: /(?:bank|reconcile|bookkeeping[_-](?:batch|workbench))/ },
  { capabilityId: "document-intake", pattern: /(?:documents?|mail[_-]intake|imap[_-]intake|bilagsmail)/ },
  { capabilityId: "supplier-purchases", pattern: /(?:expense|payable|vendor|supplier|purchase[_ -](?:case|overview))/ },
  { capabilityId: "supplier-commitments", pattern: /(?:supplier[_-]commitment|commitment|subscription|liquidity[_-]forecast_13)/ },
  { capabilityId: "customer-invoicing", pattern: /(?:invoice|customer|recurring)/ },
  { capabilityId: "exceptions-corrections", pattern: /(?:exceptions?|journal|accounting-draft|opening-balance)/ },
  { capabilityId: "imports", pattern: /(?:import|archive\/:year)/ },
  { capabilityId: "planning-reporting", pattern: /(?:report|dashboard|budget|cashflow|tax_return|tax\b|annual|accrual|compliance|obligations|multi-year)/ },
  { capabilityId: "operations-assurance", pattern: /(?:system|audit|health|ready|retention|integrity|backup|meta_about|agent[_-]capabilit|agent[_-]workflow|agent run|reg coverage|reg citations|serve|local start)/ },
  { capabilityId: "company-workspace", pattern: /(?:company|companies|workspace|accounts?|cvr|contacts|members|invitations|approval[_-]policy|^cli:init$|^http:get \/api$|^http:get \/api\/health$|^http:get \/api\/rules$|^http:get \/api\/me$)/ },
  { capabilityId: "company-knowledge", pattern: /(?:company[_-]knowledge|\/knowledge)/ },
  { capabilityId: "ownership-graph", pattern: /(?:ownership(?:[_ -](?:graph|snapshot|query|propose|review|apply|history|projection))?|ownership-graph)/ },
];

export const AGENT_DISCOVERY_COVERAGE_RULES_HASH = createHash("sha256")
  .update(canonicalJson({
    schemaVersion: "rentemester-agent-discovery-coverage-v1",
    baselines: AGENT_SURFACE_BASELINES,
    rules: CAPABILITY_RULES.map((rule) => ({ capabilityId: rule.capabilityId, pattern: rule.pattern.source })),
  }))
  .digest("hex");

export function coverageIdentity() {
  return {
    schemaVersion: "rentemester-agent-discovery-coverage-v1" as const,
    rulesHash: AGENT_DISCOVERY_COVERAGE_RULES_HASH,
    surfaceBaselines: AGENT_SURFACE_BASELINES,
  };
}

export function capabilityIdsForOperation(id: string): string[] {
  const normalized = id.toLowerCase();
  return [...new Set(CAPABILITY_RULES.filter((rule) => rule.pattern.test(normalized)).map((rule) => rule.capabilityId))];
}

function surfaceHash(ids: readonly string[]): string {
  return createHash("sha256").update([...ids].sort().join("\n")).digest("hex");
}

function sourceOperationIds(input: AgentDiscoveryCoverageInput): Record<SurfaceName, string[]> {
  return {
    mcp: input.tools.map((tool) => tool.name).sort(),
    cli: input.commands.map((command) => command.key).sort(),
    http: input.routes.map((route) => `${route.method} ${route.pattern}`).sort(),
  };
}

function bindingForOperation(id: string, input: AgentDiscoveryCoverageInput): DiscoveryOperationBinding | null {
  const capabilityIds = (input.classifyOperation ?? capabilityIdsForOperation)(id);
  if (capabilityIds.length === 0) return null;
  if (id.startsWith("mcp:")) {
    const tool = input.tools.find((item) => item.name === id.slice(4));
    if (!tool?.annotations || typeof tool.annotations.readOnlyHint !== "boolean") return null;
    const safety = tool.annotations.readOnlyHint ? "read" : tool.annotations.destructiveHint ? "destructive" : "write";
    const idempotent = tool.annotations.idempotentHint === true;
    return { id, capabilityIds, safety, idempotent, requiresActor: safety !== "read", requiresConfirmation: safety !== "read", retryClass: retryClassForOperation(id, { safety, idempotent }) };
  }
  if (id.startsWith("cli:")) {
    const command = input.commands.find((item) => item.key === id.slice(4));
    if (!command) return null;
    const safety: OperationSafety = command.mutating || command.sideEffecting ? "write" : "read";
    const idempotent = safety === "read" ? true : null;
    return { id, capabilityIds, safety, idempotent, requiresActor: command.mutating === true, requiresConfirmation: command.allowedFlags?.includes("--confirm") === true, retryClass: retryClassForOperation(id, { safety, idempotent }) };
  }
  const routeId = id.slice(5);
  const separator = routeId.indexOf(" ");
  const route = input.routes.find((item) => item.method === routeId.slice(0, separator) && item.pattern === routeId.slice(separator + 1));
  if (!route) return null;
  const safety: OperationSafety = route.effect === "read" ? "read" : "write";
  const idempotent = safety === "read" ? true : null;
  return { id, capabilityIds, safety, idempotent, requiresActor: false, requiresConfirmation: false, retryClass: retryClassForOperation(id, { safety, idempotent, external: route.effect === "external" }) };
}

export function discoverableOperationBindings(sources: LiveOperationSources): DiscoveryOperationBinding[] {
  const input: AgentDiscoveryCoverageInput = {
    tools: sources.tools ?? [],
    commands: sources.commands ?? [],
    routes: sources.routes ?? [],
  };
  const ids = sourceOperationIds(input);
  return [
    ...ids.mcp.map((id) => `mcp:${id}`),
    ...ids.cli.map((id) => `cli:${id}`),
    ...ids.http.map((id) => `http:${id}`),
  ]
    .map((id) => bindingForOperation(id, input))
    .filter((binding): binding is DiscoveryOperationBinding => binding !== null)
    .sort((a, b) => a.id.localeCompare(b.id));
}

export type AgentDiscoveryCoverageInput = {
  tools: readonly LiveTool[];
  commands: readonly DiscoveryCommand[];
  routes: readonly DiscoveryRoute[];
  workflows?: readonly AgentWorkflow[];
  capabilities?: readonly AgentCapability[];
  expectedOperationIds?: readonly string[];
  classifyOperation?: (id: string) => string[];
  standaloneOperationIds?: readonly string[];
  imageDigest?: string | null;
};

export type AgentDiscoveryCoverageReport = {
  schemaVersion: "rentemester-agent-discovery-coverage-v1";
  ok: boolean;
  catalogue: ReturnType<typeof catalogueIdentity>;
  coverageHash: string;
  imageDigest: string | null;
  counts: { mcp: number; cli: number; http: number; capabilities: number; workflows: number; bindings: number };
  bindings: DiscoveryOperationBinding[];
  errors: string[];
};

export function validateAgentDiscoveryCoverage(input: AgentDiscoveryCoverageInput): AgentDiscoveryCoverageReport {
  const capabilities = input.capabilities ?? AGENT_CAPABILITIES;
  const workflows = input.workflows ?? AGENT_WORKFLOWS;
  const capabilityIds = new Set(capabilities.map((item) => item.id));
  const workflowIds = new Set(workflows.map((item) => item.id));
  const surfaces = sourceOperationIds(input);
  const liveIds = [...surfaces.mcp.map((id) => `mcp:${id}`), ...surfaces.cli.map((id) => `cli:${id}`), ...surfaces.http.map((id) => `http:${id}`)].sort();
  const errors: string[] = [];

  if (input.expectedOperationIds) {
    const expected = new Set(input.expectedOperationIds);
    const live = new Set(liveIds);
    for (const id of liveIds) if (!expected.has(id)) errors.push(`${id}: new public operation is not in the reviewed discovery baseline; classify it and update the baseline.`);
    for (const id of expected) if (!live.has(id)) errors.push(`${id}: reviewed operation is not live; restore it or remove its discovery classification.`);
  } else {
    for (const surface of ["mcp", "cli", "http"] as const) {
      const actual = { count: surfaces[surface].length, hash: surfaceHash(surfaces[surface]) };
      const expected = AGENT_SURFACE_BASELINES[surface];
      if (actual.count !== expected.count || actual.hash !== expected.hash) {
        errors.push(`${surface}: public surface identity changed (expected ${expected.count}/${expected.hash}, got ${actual.count}/${actual.hash}); review the live registrations, capability mappings and workflows, then update AGENT_SURFACE_BASELINES.`);
      }
    }
  }

  if ((input.standaloneOperationIds?.length ?? 0) > 0) {
    errors.push(`standalone classifications are not accepted: ${input.standaloneOperationIds!.join(", ")}; link every public operation to a named capability.`);
  }

  const bindings: DiscoveryOperationBinding[] = [];
  for (const id of liveIds) {
    const binding = bindingForOperation(id, input);
    if (!binding) {
      errors.push(`${id}: no live, machine-readable capability binding; add an explicit classification rule and reviewed surface identity.`);
      continue;
    }
    for (const capabilityId of binding.capabilityIds) {
      if (!capabilityIds.has(capabilityId)) errors.push(`${id}: capability binding '${capabilityId}' does not exist in AGENT_CAPABILITIES.`);
    }
    bindings.push(binding);
  }

  // A generic annotation cannot silently upgrade a retry guarantee. Every
  // public write has one explicit class, and each stronger claim must agree
  // with its live evidence. This catches both an unreviewed natural hint and
  // stale allow-list entries when a tool changes behaviour.
  for (const tool of input.tools) {
    const id = `mcp:${tool.name}`;
    const binding = bindings.find((item) => item.id === id);
    if (!binding || binding.safety === "read") continue;
    const explicitlyIdempotent = KEY_IDEMPOTENT_MCP_OPERATIONS.has(tool.name) || NATURAL_IDEMPOTENT_MCP_OPERATIONS.has(tool.name) || EXTERNAL_PROVIDER_MCP_OPERATIONS.has(tool.name);
    if (tool.annotations?.idempotentHint === true && !explicitlyIdempotent) errors.push(`${id}: live idempotentHint has no explicit retry classification; add a reviewed natural-idempotent or external-provider-reconciled contract.`);
    if (binding.retryClass === "natural-idempotent" && tool.annotations?.idempotentHint !== true) errors.push(`${id}: natural-idempotent claim lacks live idempotentHint evidence.`);
    if (binding.retryClass === "key-idempotent" && tool.annotations?.idempotentHint === true) errors.push(`${id}: key-idempotent operation must not also claim natural idempotency.`);
  }

  for (const capability of capabilities) {
    if (capability.workflowIds.length === 0) errors.push(`capability:${capability.id}: no canonical workflow is linked.`);
    for (const workflowId of capability.workflowIds) if (!workflowIds.has(workflowId)) errors.push(`capability:${capability.id}: workflow '${workflowId}' does not exist.`);
    if (!bindings.some((binding) => binding.capabilityIds.includes(capability.id))) errors.push(`capability:${capability.id}: no live public operation is discoverable through this capability.`);
  }

  for (const workflow of workflows) {
    if (!capabilityIds.has(workflow.capabilityId)) errors.push(`workflow:${workflow.id}: capability '${workflow.capabilityId}' does not exist.`);
    const capability = capabilities.find((item) => item.id === workflow.capabilityId);
    if (!capability?.workflowIds.includes(workflow.id)) errors.push(`workflow:${workflow.id}: reverse link from capability '${workflow.capabilityId}' is missing.`);
    const stepIds = new Set(workflow.steps.map((item) => item.id));
    for (const workflowStep of workflow.steps) {
      for (const dependencyGroup of workflowStep.dependsOn) {
        for (const dependency of dependencyGroup.split("|")) if (!stepIds.has(dependency)) errors.push(`workflow:${workflow.id}/${workflowStep.id}: dangling dependency '${dependency}'.`);
      }
      const id = operationId(workflowStep.operation);
      const binding = bindings.find((item) => item.id === id);
      if (!binding) {
        errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} is not live and capability-bound.`);
        continue;
      }
      const dryRunVariant = workflowStep.operation.surface === "cli" && workflowStep.requiredArguments?.includes("--dry-run");
      const actualSafety = dryRunVariant ? "read" : binding.safety;
      if (actualSafety !== workflowStep.expectedSafety) errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} safety claim '${workflowStep.expectedSafety}' contradicts live '${actualSafety}'.`);
      if (workflowStep.operation.surface === "mcp" && binding.idempotent !== workflowStep.expectedIdempotent) errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} idempotency claim '${workflowStep.expectedIdempotent}' contradicts live '${binding.idempotent}'.`);
      if (workflowStep.operation.surface !== "http" && binding.requiresActor !== workflowStep.requiresActor) errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} actor requirement contradicts the live surface.`);
      if (workflowStep.operation.surface !== "http" && binding.requiresConfirmation !== workflowStep.requiresConfirmation) errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} confirmation requirement contradicts the live surface.`);
      if (!dryRunVariant && workflowStep.retryClass !== binding.retryClass) errors.push(`workflow:${workflow.id}/${workflowStep.id}: ${id} retry class '${workflowStep.retryClass}' contradicts live '${binding.retryClass}'. Use the canonical retry contract; do not infer key idempotency from an input field.`);
      if (actualSafety === "read" && workflowStep.retryClass !== "safe-read") errors.push(`workflow:${workflow.id}/${workflowStep.id}: read operation must use safe-read retry semantics.`);
      if (actualSafety === "destructive" && workflowStep.retryClass !== "unsafe-read-back") errors.push(`workflow:${workflow.id}/${workflowStep.id}: destructive operation requires read-back-before-retry semantics.`);
    }
  }

  const sortedBindings = bindings.sort((a, b) => a.id.localeCompare(b.id));
  const coverageHash = createHash("sha256").update(canonicalJson({ catalogueHash: AGENT_CATALOGUE_HASH, baselines: AGENT_SURFACE_BASELINES, bindings: sortedBindings, workflows: workflows.map((item) => item.id) })).digest("hex");
  return {
    schemaVersion: "rentemester-agent-discovery-coverage-v1",
    ok: errors.length === 0,
    catalogue: catalogueIdentity(),
    coverageHash,
    imageDigest: input.imageDigest ?? null,
    counts: { mcp: input.tools.length, cli: input.commands.length, http: input.routes.length, capabilities: capabilities.length, workflows: workflows.length, bindings: sortedBindings.length },
    bindings: sortedBindings,
    errors,
  };
}
