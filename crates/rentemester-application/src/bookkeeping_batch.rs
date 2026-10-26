//! One current control authorization and ONE company IMMEDIATE transaction for an entire batch.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{
    Error,
    bookkeeping_batch::{Exact, Persist, Scope, WorkbenchInput},
    fiscal_year::Date,
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::bookkeeping_batch as store;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub enum Action<'a> {
    Plan(&'a Scope),
    Persist(&'a Persist),
    Approve(&'a Exact),
    Status(i64),
    StatusByKey(&'a str),
    Apply(&'a Exact),
    Workbench(&'a WorkbenchInput),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Plan(_) | Action::Status(_) | Action::StatusByKey(_) | Action::Workbench(_) => {
            Permission::CompanyRead
        }
        Action::Persist(_) => Permission::CompanyDraftWrite,
        Action::Approve(_) => Permission::CompanyReview,
        Action::Apply(_) => Permission::CompanyLedgerPost,
    };
    let clock = context.clock;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, subject| {
            let now = clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "trusted batch clock unavailable"))?;
            let at = rentemester_engine::company_knowledge::timestamp(now)?;
            let today = rentemester_runtime::posting_time::latest_date(now, 0)
                .map_err(|e| Error::new(e.code(), "trusted Copenhagen date unavailable"))?;
            let latest = Date::parse(
                &rentemester_runtime::posting_time::latest_date(now, 45)
                    .map_err(|e| Error::new(e.code(), "posting horizon unavailable"))?,
            )?;
            match action {
                Action::Status(id) => Ok(json!({"ok":true,"state":store::status(tx,id)?})),
                Action::StatusByKey(key) => {
                    Ok(json!({"ok":true,"state":store::status_by_key(tx,key)?}))
                }
                Action::Plan(scope) => {
                    let p = store::plan_at(tx, scope, latest, &today, &at)?;
                    verify_plan(tx, guard, &p)?;
                    Ok(json!({"ok":true,"dryRun":true,"plan":p}))
                }
                Action::Persist(input) => {
                    input.validate()?;
                    let p = store::plan_at(tx, &input.scope(), latest, &today, &at)?;
                    verify_plan(tx, guard, &p)?;
                    store::persist_at(tx, input, subject, latest, &today, &at)
                }
                Action::Approve(input) => {
                    let p = store::stored_plan(tx, input)?;
                    verify_plan(tx, guard, &p)?;
                    store::approve_at(tx, input, subject, latest, &today, &at)
                }
                Action::Apply(input) => {
                    let p = store::stored_plan(tx, input)?;
                    verify_plan(tx, guard, &p)?;
                    store::apply_at(tx, input, subject, latest, &today, &at)
                }
                Action::Workbench(input) => {
                    let p = store::plan_at(tx, &input.scope(), latest, &today, &at)?;
                    verify_plan(tx, guard, &p)?;
                    let evidence = crate::periods::verify_sources(tx, guard);
                    let period = rentemester_storage::periods::compute(
                        tx,
                        &input.from,
                        &input.to,
                        evidence,
                    )?;
                    let blockers = period["blockers"].as_u64();
                    let close = match blockers {
                        Some(n) => json!({"status":"available","blockers":n,"hash":period["hash"]}),
                        None => json!({"status":"unavailable","blockers":null}),
                    };
                    store::workbench_at(tx, input, latest, &today, close, &at)
                }
            }
        },
    )
}
/// Before ANY apply effects, verify real originals and archived bank proofs under the retained guard.
pub fn verify_plan(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    plan: &Value,
) -> Result<(), Error> {
    for item in plan["items"]
        .as_array()
        .ok_or_else(|| Error::new("bookkeeping_batch_refused", "invalid complete plan"))?
        .iter()
        .filter(|i| i["partition"] == "ready")
    {
        let document = item["documentId"]
            .as_i64()
            .ok_or_else(|| Error::new("bookkeeping_batch_refused", "missing document proof"))?;
        let bank = item["bankTransactionId"]
            .as_i64()
            .ok_or_else(|| Error::new("bookkeeping_batch_refused", "missing bank proof"))?;
        let original = rentemester_storage::documents::evidence(tx, document)?;
        guard
            .read_document(&original.path, &original.hash)
            .map_err(|_| {
                Error::new(
                    "bookkeeping_batch_original_unavailable",
                    "Original purchase missing or changed",
                )
            })?;
        let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
        let bytes = guard.read_bank_statement(&hash).map_err(|_| {
            Error::new(
                "bookkeeping_batch_bank_original_unavailable",
                "Archived bank original missing or changed",
            )
        })?;
        rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
    }
    Ok(())
}
