//! Native source-verified batch contracts; metadata never plans or executes accounting.
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(super) const WORKFLOW: &str = "bank-reconciliation-batch";
pub(super) const CAPABILITY: &str = "bank-bookkeeping";
pub(super) fn capability() -> Value {
    json!({"id":CAPABILITY,"title":"Bank bookkeeping and independently reviewed batches","purpose":"Import and inspect retained bank evidence, review a complete source-bound plan, approve independently and apply an atomic native purchase batch.","domain":"bank","outcomes":["retain bank statement evidence","inspect unmatched rows","apply reviewed atomic bookkeeping batch","reconcile durable batch status"],"keywords":["bank","bookkeeping","batch","reconciliation","reconcile","statement","import","bankafstemning","bogføring"],"scope":"company","supportStatus":"partial","maturity":"limited","workflowIds":[WORKFLOW],"canonicalState":["bank transactions","original documents","bookkeeping batch revisions and approvals","item receipts","journal entries"],"unsupportedBoundaries":[super::PARTIAL,"Native bank supports optional fiscal year only, not original bank_list filters/pagination or bank_import aliases.","No arbitrary unmatched row, foreign purchase family, implicit account/rule, automatic approval, payment transfer or legacy partial resume is promised."]})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn company() -> Value {
    json!({"type":"string","minLength":1,"pattern":"^[A-Za-z0-9_-]+$"})
}
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
}
fn scope() -> Value {
    let date = json!({"type":"string","minLength":10,"maxLength":10,"description":"Explicit real YYYY-MM-DD; both ranges must be ordered."});
    object(
        json!({"accountingFrom":date,"accountingTo":date,"bankFrom":date,"bankTo":date,"companyId":id()}),
        &["accountingFrom", "accountingTo", "bankFrom", "bankTo"],
    )
}
fn exact() -> Value {
    object(
        json!({"runId":id(),"planHash":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"},"confirm":{"const":true}}),
        &["runId", "planHash", "confirm"],
    )
}
fn status_schema() -> Value {
    let mut value = object(
        json!({"company":company(),"runId":id(),"runKey":{"type":"string","minLength":1,"maxLength":1024}}),
        &["company"],
    );
    value["oneOf"] = json!([{"required":["runId"]},{"required":["runKey"]}]);
    value
}
fn operation(method: &str, path: &str) -> Value {
    let mut value = super::operation(method, path);
    value["queryParameters"] = json!({});
    value.as_object_mut().unwrap().remove("optionalArguments");
    value
}
fn step(
    id: &str,
    dependencies: &[&str],
    write: bool,
    tool: &str,
    cli: &str,
    path: &str,
    purpose: &str,
) -> Value {
    let mut value = super::step(id, dependencies, write, tool, cli, path);
    value["operation"] = operation(if write { "POST" } else { "GET" }, path);
    value["purpose"] = json!(purpose);
    value["authority"] = json!({"permission":"CompanyRead","identity":"Current authenticated human MFA for this complete journey; authority is rechecked by the server, never supplied as actor/body fields."});
    value["requiredArguments"] = json!(["company"]);
    value["mcpInputSchema"] = object(json!({"company":company()}), &["company"]);
    value["canonicalRecords"] = json!([
        "bank transactions",
        "original documents",
        "bookkeeping batch revisions/approvals/receipts",
        "journal entries"
    ]);
    value
}
fn write_contract(value: &mut Value, body: Value, permission: &str, max: usize) {
    value["requiredArguments"] = json!(["company", "body", "confirm:true"]);
    value["bodySchema"] = body.clone();
    value["mcpInputSchema"] = object(
        json!({"company":company(),"body":{"type":"string","minLength":2,"maxLength":max,"contentMediaType":"application/json","contentSchema":body},"confirm":{"const":true}}),
        &["company", "body", "confirm"],
    );
    value["encoding"] = json!(
        "HTTP application/json object; MCP body is the exact JSON string plus outer confirm:true; CLI exact stdin JSON plus --confirm. No actor/key/hash/default injection or automatic retry."
    );
    value["authority"]["permission"] = json!(permission);
}
pub(super) fn workflow() -> Value {
    let mut accounts = step(
        "inspect-bank-accounts",
        &[],
        false,
        "bank_accounts",
        "remote bank-accounts",
        "/api/companies/:slug/bank-accounts",
        "Inspect actual account IDs/slugs, currency, ledger mapping and declared import profiles.",
    );
    accounts["outputIdentities"] = json!([
        "bank account id/slug",
        "ledger account mapping",
        "supported import profile"
    ]);
    let mut import = step(
        "import-bank",
        &["inspect-bank-accounts"],
        true,
        "import_bank",
        "remote import-bank",
        "/api/companies/:slug/bank/import",
        "Retain original CSV and imported row identities without posting journals; skip a new import when that exact source is already retained.",
    );
    let mut import_body = object(
        json!({"confirm":{"const":true},"csvContent":{"type":"string","minLength":1,"maxLength":26214400},"csvBase64":{"type":"string","minLength":4,"maxLength":34952536},"account":{"type":["string","null"],"maxLength":128},"profile":{"type":["string","null"],"enum":[null,"","danske-bank"]}}),
        &["confirm"],
    );
    import_body["oneOf"] = json!([{"required":["csvContent"],"not":{"required":["csvBase64"]}},{"required":["csvBase64"],"not":{"required":["csvContent"]}}]);
    write_contract(
        &mut import,
        import_body,
        "CompanyLedgerPost",
        35 * 1024 * 1024,
    );
    import["authority"]["identity"] = json!(
        "Current human MFA or explicitly granted current bank-import service identity. Service import access does not authorize the human-only batch steps."
    );
    import["prerequisites"] = json!([
        "Select an existing active account ID/slug from bank_accounts and verify its ledger mapping.",
        "Supply exactly one UTF-8 csvContent or canonical standard csvBase64 retaining raw bytes, at most25MiB original/35MiB JSON.",
        "Generic omitted/null/empty profile uses UTF-8/BOM with unambiguous ISO dates; danske-bank selects declared semicolon/day-month-year UTF-8 or Windows1252 fallback.",
        "No generic idempotency key. After uncertainty inspect retained bank rows/source identities before deciding whether an import occurred; do not automatically reimport."
    ]);
    import["outputIdentities"] = json!(["import source/duplicate result", "bank transaction IDs"]);
    import["uncertainOutcomeReadBack"] = operation("GET", "/api/companies/:slug/bank");
    let mut bank = step(
        "inspect-bank",
        &["import-bank"],
        false,
        "bank",
        "remote bank",
        "/api/companies/:slug/bank",
        "Inspect imported rows, amounts, currency, dates, source/reconciliation identity and journal references.",
    );
    bank["mcpInputSchema"] = object(
        json!({"company":company(),"year":{"type":"string","maxLength":128}}),
        &["company"],
    );
    bank["operation"]["querySchema"] =
        object(json!({"year":{"type":"string","maxLength":128}}), &[]);
    bank["cliArguments"] = json!(["--year FISCAL_LABEL (optional)"]);
    bank["prerequisites"] = json!([
        "Select the server fiscal label covering the intended bank dates; repeat for every label touched by the scope. Omitted year is server-selected, not an all-years promise.",
        "Native bank has no from/to/status/page filters. Preserve exact stored row/source identities and missing balance status; never manufacture a zero balance."
    ]);
    bank["outputIdentities"] = json!([
        "bankTransactionId",
        "original statement/source identity",
        "reconciliation/journal identity"
    ]);
    let mut advice = step(
        "inspect-suggestions",
        &["inspect-bank"],
        false,
        "bank_suggest_matches",
        "remote bank-suggest-matches",
        "/api/companies/:slug/bank/match-suggestions",
        "Inspect server-scored advice only; a suggestion or equal amount never grants accounting approval.",
    );
    let suggestion = object(
        json!({"company":company(),"bankTransactionId":id(),"max":{"type":"integer","minimum":1}}),
        &["company"],
    );
    advice["mcpInputSchema"] = suggestion;
    advice["operation"]["querySchema"] = object(
        json!({"bankTransactionId":id(),"max":{"type":"integer","minimum":1}}),
        &[],
    );
    advice["cliArguments"] = json!(["--bank-transaction-id ID (optional)", "--max N (optional)"]);
    advice["prerequisites"] = json!([
        "bankTransactionId optionally selects one unmatched row; max defaults5 and limits suggestions per row, not rows. Scores can exceed1 and are not probabilities. Preserve canonical ok:false/errors."
    ]);
    let mut queue = step(
        "inspect-source-queue",
        &["inspect-suggestions"],
        false,
        "reviewed_bank",
        "remote reviewed-bank",
        "/api/companies/:slug/reviewed-bank",
        "Inspect actual unposted document/bank evidence and expectedSourceHash; hints are advisory, not independent rule or batch approval.",
    );
    queue["prerequisites"] = json!([
        "Retained immutable purchase originals and archived statement bytes must be available and match hashes.",
        "Native batch ready items require a unique corroborated outgoing DKK match, resolved Danish supplier and supported DKK purchase/receipt, applicable independently reviewed purchase/VAT rule, accounts/roles/dimensions and current source checks. Missing or ambiguous items require human resolution."
    ]);
    let mut plan = step(
        "plan-batch",
        &["inspect-source-queue"],
        false,
        "bookkeeping_batch_plan",
        "remote bookkeeping-batch-plan",
        "/api/companies/:slug/bookkeeping-batch",
        "Read the complete candidate universe, partitions, source identities, candidateSetHash and exact planHash without persistence.",
    );
    plan["mcpInputSchema"] = object(
        json!({"company":company(),"scope":scope()}),
        &["company", "scope"],
    );
    plan["requiredArguments"] = json!([
        "company",
        "scope.accountingFrom",
        "scope.accountingTo",
        "scope.bankFrom",
        "scope.bankTo"
    ]);
    plan["operation"]["querySchema"] = scope();
    plan["cliArguments"] = json!([
        "--accounting-from DATE",
        "--accounting-to DATE",
        "--bank-from DATE",
        "--bank-to DATE"
    ]);
    plan["prerequisites"] = json!([
        "Use four explicit real ordered ISO dates; no inferred period/default scope. Optional companyId must match the selected admitted company and is HTTP/SDK/MCP only; the CLI exposes no companyId flag.",
        "Inspect every partition and actual complete source set; no truncated plan, skipped unknown row or client-computed hash/account/amount. Suggestions and reviewed-bank are not substitutes for this plan."
    ]);
    plan["outputIdentities"] = json!([
        "plan.planHash",
        "plan.candidateSetHash",
        "sourceIdentities",
        "items.actionKey/evidenceHash",
        "executionPolicy:atomic_v1"
    ]);
    let mut persist = step(
        "persist-batch",
        &["plan-batch"],
        true,
        "bookkeeping_batch_persist",
        "remote bookkeeping-batch-persist",
        "/api/companies/:slug/bookkeeping-batch/persist",
        "Persist the exact reviewed complete scope/hash as an immutable revision bound to caller runKey and authenticated planner.",
    );
    let mut persist_body = scope();
    persist_body["properties"]["runKey"] = json!({"type":"string","minLength":1,"maxLength":1024});
    persist_body["properties"]["expectedPlanHash"] = exact()["properties"]["planHash"].clone();
    persist_body["properties"]["confirm"] = json!({"const":true});
    persist_body["required"].as_array_mut().unwrap().extend([
        json!("runKey"),
        json!("expectedPlanHash"),
        json!("confirm"),
    ]);
    write_contract(&mut persist, persist_body, "CompanyDraftWrite", 32768);
    persist["prerequisites"] = json!([
        "Current human MFA CompanyDraftWrite. Supply the same four scope dates, explicit stable nonblank runKey and exact reviewed plan.planHash as expectedPlanHash.",
        "Server recomputes and compares the complete plan before persistence. Changed evidence/hash refuses; never substitute a new hash, runKey or scope without fresh explicit review."
    ]);
    persist["outputIdentities"] = json!([
        "runId",
        "stored planHash/revision",
        "planner subject",
        "runKey"
    ]);
    persist["uncertainOutcomeReadBack"] =
        operation("GET", "/api/companies/:slug/bookkeeping-batch/runs");
    persist["uncertainOutcomeReadBack"]["queryParameters"] =
        json!({"runKey":"exact original caller runKey"});
    persist["replayContract"] = json!(
        "Same runKey and plan hash can return duplicate:true after current plan validation; the key cannot bind another plan. Read status by that original runKey first, never generate a replacement key or automatically resend."
    );
    let mut stored = step(
        "inspect-stored-run",
        &["persist-batch"],
        false,
        "bookkeeping_batch_status",
        "remote bookkeeping-batch-status",
        "/api/companies/:slug/bookkeeping-batch/runs/:runId",
        "Read canonical immutable revision and planner identity for independent review; preserve stored runId and planHash.",
    );
    stored["mcpInputSchema"] = status_schema();
    stored["requiredArguments"] = json!(["company", "exactly one runId or runKey"]);
    stored["cliArguments"] = json!(["--id RUN_ID OR --run-key EXACT_ORIGINAL_KEY"]);
    stored["operation"]["pathParameters"] =
        json!({"runId":"positive exact run ID from persistence/status"});
    stored["outputIdentities"] = json!([
        "run/revisions",
        "planner/approver subjects",
        "attempts",
        "receipts",
        "finalChecks"
    ]);
    let mut approve = step(
        "approve-batch",
        &["inspect-stored-run"],
        true,
        "bookkeeping_batch_approve",
        "remote bookkeeping-batch-approve",
        "/api/companies/:slug/bookkeeping-batch/approve",
        "A distinct current human reviewer approves the same immutable stored revision; the planner cannot approve their own revision.",
    );
    write_contract(&mut approve, exact(), "CompanyReview", 32768);
    approve["prerequisites"] = json!([
        "Use a current human MFA session with CompanyReview and a subject different from the stored planner.",
        "Review canonical stored plan/source/rule/VAT proofs; send exactly runId, planHash and confirm:true. Approval checks the complete current plan equals the stored revision; no silent replan."
    ]);
    approve["inputIdentities"] = json!([
        "stored runId",
        "stored planHash",
        "distinct reviewer subject"
    ]);
    approve["outputIdentities"] = json!(["exact revision approval", "approver subject"]);
    approve["uncertainOutcomeReadBack"] = stored["operation"].clone();
    approve["replayContract"] = json!(
        "Repeated same-reviewer approval of that exact revision can read existing approval; this is not permission for automatic retry. Read status first."
    );
    let mut apply = step(
        "apply-batch",
        &["approve-batch"],
        true,
        "bookkeeping_batch_apply",
        "remote bookkeeping-batch-apply",
        "/api/companies/:slug/bookkeeping-batch/apply",
        "Apply the same independently approved stored plan atomically; never execute a newly recomputed unreviewed plan.",
    );
    write_contract(&mut apply, exact(), "CompanyLedgerPost", 32768);
    apply["inputIdentities"] = json!(["same approved runId", "same approved planHash"]);
    apply["outputIdentities"] = json!([
        "applyAttemptId",
        "actionKey/planHash/evidenceHash item receipts",
        "journalEntryId/journalEntryHash",
        "purchase/rule/VAT/dimension proof IDs",
        "mandatory final checks"
    ]);
    apply["prerequisites"] = json!([
        "Current human MFA CompanyLedgerPost and independent approval of this exact revision. Applicator authority never substitutes for reviewer independence.",
        "Server verifies retained original purchase and archived bank bytes, current complete candidate set, rules/configuration/periods, role/account mapping, namespace/transfer fences and configured backup lock. Source changes require a newly reviewed revision, not silent replan.",
        "atomic_v1 posts ready items inside one admitted company transaction. Audit-chain, trial-balance, exact-range reconciliation and VAT final checks must all succeed; any refusal rolls back the entire batch. Unresolved rows can prevent final reconciliation; do not skip them."
    ]);
    apply["uncertainOutcomeReadBack"] = stored["operation"].clone();
    apply["replayContract"] = json!(
        "Exact runId+planHash replay can return verified complete immutable receipts with duplicate:true under current authority/source checks. Incomplete/conflicting/legacy partial receipts never authorize resume. Read status/bank/journal/originals first; no automatic resend or new key."
    );
    let mut status = stored.clone();
    status["id"] = json!("verify-batch-status");
    status["dependsOn"] = json!(["apply-batch"]);
    status["purpose"] = json!(
        "Inspect durable approvals, attempts, complete receipts and all four successful final checks after success or uncertainty."
    );
    let mut verify_bank = bank.clone();
    verify_bank["id"] = json!("verify-bank");
    verify_bank["dependsOn"] = json!(["verify-batch-status"]);
    verify_bank["purpose"] = json!(
        "Reconcile actual bank-to-journal relations using every relevant fiscal-year label, not a guessed current-year default."
    );
    let mut journal = step(
        "verify-journal",
        &["verify-batch-status"],
        false,
        "journal",
        "remote journal",
        "/api/companies/:slug/journal",
        "Read explicit live canonical journals and match every receipt's journalEntryId, source identities and immutable evidence.",
    );
    journal["mcpInputSchema"] = object(
        json!({"company":company(),"year":{"type":"string","maxLength":128},"account":{"type":"string","maxLength":128},"source":{"enum":["live","archive"]}}),
        &["company"],
    );
    journal["operation"]["queryParameters"] = json!({"source":"live"});
    journal["readArguments"] = json!({"source":"live"});
    journal["cliArguments"] = json!([
        "--source live",
        "--year FISCAL_LABEL (repeat for each relevant label)"
    ]);
    journal["prerequisites"] = json!([
        "Use source=live and the fiscal label(s) containing the stored scope; archive/default selection is not proof of a newly posted native journal. Do not invent unsupported journal date filters."
    ]);
    let mut value = json!({"id":WORKFLOW,"capabilityId":CAPABILITY,"title":"Native bank reconciliation batch","intendedOutcome":"Retain bank source, inspect evidence, persist one exact source-bound plan, approve independently, atomically apply and reconcile durable status.","nonGoals":["Discovery does not execute or authorize anything.","No legacy bank_list/bank_import aliases, arbitrary foreign purchase support, automatic approval/payment or implicit actor/path/environment behavior."],"prerequisites":["Verify running artifact and connected endpoint; resolve every actual alias/route.","Choose one explicit admitted company and current human MFA identity for the complete batch journey.","Retain purchase originals, archived bank bytes, confirmed mappings and independently reviewed applicable purchase/VAT rules."],"steps":[accounts,import,bank,advice,queue,plan,persist,stored,approve,apply,status,verify_bank,journal]});
    value["blockers"] = json!([{"code":"AUTHORITY_OR_CONFIRMATION","meaning":"Obtain current human permission and explicit confirmation; planner self-approval is refused."},{"code":"SOURCE_OR_PLAN_CHANGED","meaning":"Missing/changed originals, universe, period/rule/mapping facts require explicit review of a fresh plan."},{"code":"BUSINESS_PRECONDITION","meaning":"Resolve ambiguous/unsupported sources, backup lock, closed periods or final-check refusal; never weaken accounting."}]);
    value["recovery"] = json!([
        "Unknown import: read bank/source identities before any deliberate further action; do not automatically reimport.",
        "Unknown persist: bookkeeping_batch_status with exact original runKey; adopt canonical runId and stored hash, never invent a new key.",
        "Unknown approve/apply: read status by exact runId, approval subjects, attempts, item receipts and final checks; reconcile bank/live journals/originals before any manual action.",
        "Only complete verified atomic_v1 receipts support exact identity-preserving replay; discovery never authorizes an automatic retry.",
        "Legacy partial/unversioned runs are inspect-only, never automatically resume. Correct accepted accounting through explicit correction/reversal controls."
    ]);
    value["stopConditions"] = json!([
        "Source/authority/confirmation/independent review is missing.",
        "Plan/hash/evidence changed or an advertised operation is unavailable.",
        "Any write outcome remains uncertain after canonical reconciliation.",
        "Requested behavior exceeds the finite native contract, including original bank filters or implicit actor/path/env behavior."
    ]);
    value["relatedWorkflowIds"] = json!([super::WORKFLOW]);
    value["alternatives"] = json!([]);
    value["unsupportedBoundaries"] = json!([
        super::PARTIAL,
        "Full original R44/R45 alias/filter/actor/default-launcher/envelope coverage remains open; native atomic batch execution is not generic partial-resume."
    ]);
    value["live"] = Value::Null;
    value["unresolvedOperations"] = json!([]);
    value["resolutionReason"] = json!(
        "Pure metadata cannot attest the running registry or authorize current company action; resolve with the selected live transport."
    );
    value
}
pub(super) fn operations() -> Value {
    let workflow = workflow();
    let mut seen = BTreeSet::new();
    let mut result = vec![];
    for step in workflow["steps"].as_array().unwrap() {
        for key in ["operation", "uncertainOutcomeReadBack"] {
            if let Some(operation) = step.get(key)
                && seen.insert(operation["id"].as_str().unwrap().to_owned())
            {
                result.push(operation.clone());
            }
        }
    }
    json!(result)
}
