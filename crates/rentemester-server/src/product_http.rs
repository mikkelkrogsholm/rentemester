//! Thin browser adapter over the same owner used for authentication.
mod saft;
mod company_knowledge;
mod workspace_inbox;
mod parties;
mod corporate_records;
mod workspace_access;
mod company_creation;
mod accountant;
mod reminder_claims;
mod accruals;
mod assets;
mod bank;
mod bank_reconciliation;
mod budget;
mod company_setup;
mod documents;
mod gdpr;
mod dimensions;
mod exceptions;
mod dashboard;
mod mileage;
mod integrity;
mod multi_year;
mod annual_report;
mod obligations;
mod payables;
mod periods;
mod portable_backup;
mod workspace_transfer;
mod restored_receipts;
mod recurring_invoices;
mod scheduled_recurring;
mod hosted_recurring;
mod reviewed_bookkeeping;
mod bookkeeping_batch;
mod payable_correction;
mod document_party_links;
mod sales;
mod invoice_email;
mod supplier_commitments;
mod vat;
use crate::backend::Backend;
use axum::{
    Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rentemester_application::{
    control_fence::FailureStage, journal_post, journal_reversal, owned_report::CockpitRequest,
};
use rentemester_runtime::auth::{SystemClock, VerifiedCookie};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
struct Product {
    native: Arc<Backend>,
    origin: String,
    authority: String,
    workers: Arc<Semaphore>,
}
pub fn router(native: Arc<Backend>, origin: &str) -> Router {
    let state = Product {
        native,
        origin: origin.into(),
        authority: origin.split_once("://").unwrap().1.into(),
        // The registry starts five independent reads; retain bounded headroom
        // for its normal reads and the current-session check.
        workers: Arc::new(Semaphore::new(8)),
    };
    Router::new()
        .route("/api/companies/{slug}/payables/direct-bank-correction/plan", post(payable_correction::handle))
        .route("/api/companies/{slug}/payables/direct-bank-correction/apply", post(payable_correction::handle))
        .route("/api/group-eliminations", get(eliminations::overview))
        .route("/api/companies/{slug}/eliminations", get(eliminations::handle))
        .route("/api/companies/{slug}/eliminations/{action}", post(eliminations::handle))
        .route("/api/group-report-profiles", get(consolidation::read))
        .route("/api/group-consolidated-report", get(consolidation::read))
        .route("/api/companies/{slug}/consolidation", get(consolidation::handle))
        .route("/api/companies/{slug}/consolidation/{action}", post(consolidation::handle))
        .route("/api/group-dispositions/{id}", get(intercompany_dispositions::status))
        .route("/api/companies/{slug}/group-dispositions/{action}", post(intercompany_dispositions::handle))
        .route("/api/group-reconciliation", get(intercompany::reconciliation))
        .route("/api/companies/{slug}/intercompany", get(intercompany::handle))
        .route("/api/companies/{slug}/intercompany/{action}", post(intercompany::handle))
        .route("/api/companies/{slug}/workspace-inbox", get(workspace_inbox::handle).post(workspace_inbox::handle))
        .route("/api/companies/{slug}/workspace-inbox/{source}", get(workspace_inbox::handle))
        .route("/api/companies/{slug}/workspace-inbox/{source}/assign", post(workspace_inbox::handle))
        .route("/api/companies/{slug}/workspace-inbox/{source}/complete", post(workspace_inbox::handle))
        .route("/api/workspace/backup", post(portable_backup::handle))
        .route("/api/workspace/transfer", get(workspace_transfer::handle))
        .route("/api/workspace/transfer/{action}", post(workspace_transfer::handle))
        .route("/api/companies/{slug}/restore/receipts", get(restored_receipts::handle))
        .route("/api/companies/{slug}/restore/receipts/reconcile", post(restored_receipts::handle))
        .route("/api/workspace/invitations",get(workspace_access::handle).post(workspace_access::handle))
        .route("/api/workspace/invitations/cancel",post(workspace_access::handle))
        .route("/api/invitations/claim",post(workspace_access::handle))
        .route("/api/workspace/members",get(workspace_access::handle))
        .route("/api/workspace/members/access",post(workspace_access::handle))
        .route("/api/workspace/members/company",post(workspace_access::handle))
        .route(
            "/api/companies/{slug}/budget",
            get(budget::handle).post(budget::handle),
        )
        .route("/api/companies/{slug}/budget-dimension-actuals", get(budget::handle))
        .route(
            "/api/companies/{slug}/budget-vs-actual",
            get(budget::handle),
        )
        .route("/api/companies/{slug}/documents/party-links", get(document_party_links::handle))
        .route("/api/companies/{slug}/documents/{id}/party-links", get(document_party_links::handle))
        .route("/api/companies/{slug}/documents/party-links/{action}", post(document_party_links::handle))
        .route("/api/companies/{slug}/bookkeeping-batch", get(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-batch/persist", post(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-batch/approve", post(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-batch/apply", post(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-batch/runs", get(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-batch/runs/{runId}", get(bookkeeping_batch::handle))
        .route("/api/companies/{slug}/bookkeeping-workbench", get(bookkeeping_batch::handle))
        .route(
            "/api/companies/{slug}/accounting-drafts",
            get(reviewed_bookkeeping::handle).post(reviewed_bookkeeping::handle),
        )
        .route(
            "/api/companies/{slug}/accounting-drafts/{id}/{action}",
            post(reviewed_bookkeeping::handle),
        )
        .route(
            "/api/companies/{slug}/posting-rules",
            get(reviewed_bookkeeping::handle),
        )
        .route(
            "/api/companies/{slug}/posting-rules/{action}",
            post(reviewed_bookkeeping::handle),
        )
        .route(
            "/api/companies/{slug}/reviewed-bank",
            get(reviewed_bookkeeping::handle).post(reviewed_bookkeeping::handle),
        )
        .route("/api/companies/{slug}/workspace-parties",get(parties::handle).post(parties::handle))
        .route("/api/companies/{slug}/workspace-parties/{id}",get(parties::handle))
        .route("/api/companies/{slug}/workspace-parties/{id}/role",post(parties::handle))
        .route("/api/companies/{slug}/workspace-parties/{id}/assertion",post(parties::handle))
        .route("/api/companies/{slug}/workspace-parties/{id}/alias",post(parties::handle))
        .route("/api/companies/{slug}/knowledge",get(company_knowledge::handle))
        .route("/api/companies/{slug}/knowledge/propose",post(company_knowledge::handle))
        .route("/api/companies/{slug}/knowledge/review",post(company_knowledge::handle))
        .route("/api/companies/{slug}/knowledge/supersede",post(company_knowledge::handle))
        .route("/api/companies", get(companies).post(company_creation::handle))
        .route("/api/companies/{slug}/retention", get(gdpr::handle))
        .route("/api/companies/{slug}/gdpr/export", post(gdpr::handle))
        .route("/api/companies/{slug}/gdpr/erase", post(gdpr::handle))
        .route("/api/companies/{slug}/dimensions", get(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/members", get(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/{line}", get(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/define", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/member", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/definition-lifecycle", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/member-lifecycle", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/plan", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/apply", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/replace", post(dimensions::handle))
        .route("/api/companies/{slug}/dimensions/supersede", post(dimensions::handle))
        .route("/api/companies/{slug}/multi-year", get(multi_year::read))
        .route("/api/companies/{slug}/annual-report", get(annual_report::read))
        .route("/api/companies/{slug}/obligations", get(obligations::read))
        .route("/api/companies/{slug}/exceptions", get(exceptions::handle))
        .route(
            "/api/companies/{slug}/exceptions/{id}/resolve",
            post(exceptions::handle),
        )
        .route(
            "/api/companies/{slug}/accountant-export",
            post(accountant::export),
        )
        .route("/api/companies/{slug}/saft-export", post(saft::export))
        .route("/api/companies/{slug}/invoices/{id}/reminder-claims",get(reminder_claims::handle))
        .route("/api/companies/{slug}/invoices/reminders/register",post(reminder_claims::handle))
        .route("/api/companies/{slug}/invoices/reminders/post",post(reminder_claims::handle))
        .route("/api/companies/{slug}/invoices/claims/settle",post(reminder_claims::handle))
        .route("/api/companies/{slug}/overview", get(dashboard::read))
        .route(
            "/api/companies/{slug}/mileage",
            get(mileage::handle).post(mileage::handle),
        )
        .route("/api/companies/{slug}/scheduled-recurring/{id}/hosted-plan", get(hosted_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/hosted-consent", post(hosted_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/hosted-revoke", post(hosted_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring", get(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/{id}/plan", get(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/enable", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/reconsent", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/pause", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/resume", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/cancel", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/catch-up", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/skip", post(scheduled_recurring::handle))
        .route("/api/companies/{slug}/scheduled-recurring/dispatch", post(scheduled_recurring::handle))
        .route(
            "/api/companies/{slug}/recurring-invoices",
            get(recurring_invoices::handle).post(recurring_invoices::handle),
        )
        .route(
            "/api/companies/{slug}/recurring-invoices/{id}/generate",
            post(recurring_invoices::handle),
        )
        .route(
            "/api/companies/{slug}/recurring-invoices/{id}/retire",
            post(recurring_invoices::handle),
        )
        .route("/api/companies/{slug}/ownership", get(ownership::handle))
        .route("/api/companies/{slug}/ownership/history", get(ownership::handle))
        .route("/api/companies/{slug}/ownership/propose", post(ownership::handle))
        .route("/api/companies/{slug}/ownership/review", post(ownership::handle))
        .route("/api/companies/{slug}/ownership/apply", post(ownership::handle))
        .route("/api/companies/{slug}/corporate-records",get(corporate_records::handle).post(corporate_records::handle))
        .route("/api/companies/{slug}/corporate-records/{id}",get(corporate_records::handle))
        .route("/api/companies/{slug}/corporate-records/{id}/download",get(corporate_records::handle))
        .route("/api/companies/{slug}/corporate-records/{id}/link",post(corporate_records::handle))
        .route("/api/companies/{slug}/corporate-records/{id}/enrich",post(corporate_records::handle))
        .route("/api/companies/{slug}/corporate-records/{id}/supersede",post(corporate_records::handle))
        .route(
            "/api/companies/{slug}/accruals",
            get(accruals::handle).post(accruals::handle),
        )
        .route(
            "/api/companies/{slug}/accruals/{id}/schedule",
            get(accruals::handle),
        )
        .route(
            "/api/companies/{slug}/accruals/{id}/recognize",
            post(accruals::handle),
        )
        .route(
            "/api/companies/{slug}/assets",
            get(assets::handle).post(assets::handle),
        )
        .route(
            "/api/companies/{slug}/assets/{id}/next-depreciation",
            get(assets::handle),
        )
        .route(
            "/api/companies/{slug}/assets/{id}/depreciate",
            post(assets::handle),
        )
        .route(
            "/api/companies/{slug}/supplier-commitments",
            get(supplier_commitments::handle).post(supplier_commitments::handle),
        )
        .route(
            "/api/companies/{slug}/supplier-commitments/plan",
            post(supplier_commitments::handle),
        )
        .route(
            "/api/companies/{slug}/supplier-commitments/change",
            post(supplier_commitments::handle),
        )
        .route(
            "/api/companies/{slug}/supplier-commitments/match",
            post(supplier_commitments::handle),
        )
        .route(
            "/api/companies/{slug}/supplier-commitments/matches",
            get(supplier_commitments::handle),
        )
        .route("/api/companies/{slug}/periods", get(periods::handle))
        .route(
            "/api/companies/{slug}/periods/report",
            post(periods::handle),
        )
        .route(
            "/api/companies/{slug}/periods/close-readiness",
            get(periods::handle),
        )
        .route(
            "/api/companies/{slug}/periods/close-review",
            post(periods::handle),
        )
        .route("/api/companies/{slug}/periods/close", post(periods::handle))
        .route(
            "/api/companies/{slug}/periods/reopen",
            post(periods::handle),
        )
        .route("/api/companies/{slug}/contacts", get(sales::handle))
        .route(
            "/api/companies/{slug}/payables",
            get(payables::handle).post(payables::handle),
        )
        .route(
            "/api/companies/{slug}/payables/{id}/pay",
            post(payables::handle),
        )
        .route("/api/companies/{slug}/customers", post(sales::handle))
        .route("/api/companies/{slug}/vendors", post(sales::handle))
        .route(
            "/api/companies/{slug}/vendors/{id}",
            axum::routing::patch(sales::handle).delete(sales::handle),
        )
        .route(
            "/api/companies/{slug}/customers/{id}",
            axum::routing::patch(sales::handle).delete(sales::handle),
        )
        .route("/api/companies/{slug}/invoices/email-deliveries", get(invoice_email::handle))
        .route("/api/companies/{slug}/invoices/send-email", post(invoice_email::handle))
        .route("/api/companies/{slug}/invoices/email-deliveries/reconcile", post(invoice_email::handle))
        .route("/api/companies/{slug}/invoices/email-deliveries/retry", post(invoice_email::handle))
        .route("/api/companies/{slug}/invoices", get(sales::handle))
        .route("/api/companies/{slug}/invoices/issue", post(sales::handle))
        .route(
            "/api/companies/{slug}/invoices/preview",
            post(sales::handle),
        )
        .route("/api/companies/{slug}/invoices/post", post(sales::handle))
        .route("/api/companies/{slug}/invoices/settle", post(sales::handle))
        .route("/api/companies/{slug}/invoices/refund", post(sales::handle))
        .route(
            "/api/companies/{slug}/invoices/credit-note",
            post(sales::handle),
        )
        .route(
            "/api/companies/{slug}/invoices/{id}/pdf",
            get(sales::handle),
        )
        .route(
            "/api/companies/{slug}/accounts",
            get(company_setup::handle).post(company_setup::handle),
        )
        .route(
            "/api/companies/{slug}/company",
            get(company_setup::handle).patch(company_setup::handle),
        )
        .route(
            "/api/companies/{slug}/account-roles",
            post(company_setup::handle),
        )
        .route("/api/companies/{slug}/bank/reconciliation-candidates", get(bank_reconciliation::handle))
        .route("/api/companies/{slug}/bank/reconciliation-link", post(bank_reconciliation::handle))
        .route("/api/companies/{slug}/bank/reconciliation-correction-plan", get(bank_reconciliation::handle))
        .route("/api/companies/{slug}/bank/reconciliation-correction", post(bank_reconciliation::handle))
        .route("/api/companies/{slug}/bank", get(bank::handle))
        .route(
            "/api/companies/{slug}/bank-accounts",
            get(bank::handle).post(bank::handle),
        )
        .route("/api/companies/{slug}/bank/import", post(bank::handle))
        .route("/api/companies/{slug}/documents", get(documents::list))
        .route(
            "/api/companies/{slug}/documents/ingest",
            post(documents::ingest),
        )
        .route(
            "/api/companies/{slug}/documents/{id}/file",
            get(documents::file),
        )
        .route(
            "/api/companies/{slug}/documents/{id}/booking-options",
            get(documents::expense),
        )
        .route(
            "/api/companies/{slug}/documents/book-expense",
            post(documents::expense),
        )
        .route("/api/companies/{slug}/vat", get(vat::read))
        .route("/api/companies/{slug}/vat/export", get(vat::read))
        .route("/api/companies/{slug}/integrity", get(integrity::read))
        .route("/api/companies/{slug}/journal", get(read))
        .route("/api/companies/{slug}/trial-balance", get(read))
        .route("/api/companies/{slug}/income-statement", get(read))
        .route("/api/companies/{slug}/balance", get(read))
        .route("/api/companies/{slug}/{report}/export", get(read))
        .route("/api/companies/{slug}/fiscal-years", get(read))
        .route("/api/companies/{slug}/manual-journal", post(write))
        .route("/api/companies/{slug}/manual-journal-reversal", post(write))
        .with_state(state)
}
fn response(status: StatusCode, value: Value) -> Response {
    (
        status,
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
        ],
        value.to_string(),
    )
        .into_response()
}
fn refused(status: StatusCode, code: &str) -> Response {
    response(status, json!({"ok":false,"code":code,"errors":[code]}))
}
fn one<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let mut values = headers.get_all(key).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        None
    } else {
        Some(value)
    }
}
fn proof(
    state: &Product,
    request: &Request,
    mutation: bool,
) -> Result<VerifiedCookie, Box<Response>> {
    let headers = request.headers();
    if one(headers, "host") != Some(&state.authority)
        || (mutation || headers.contains_key("origin"))
            && one(headers, "origin") != Some(&state.origin)
        || headers.contains_key("sec-fetch-site")
            && !matches!(
                one(headers, "sec-fetch-site"),
                Some("same-origin" | "same-site" | "none")
            )
    {
        return Err(Box::new(refused(StatusCode::FORBIDDEN, "origin_refused")));
    }
    if headers.contains_key("authorization") || headers.contains_key("x-rentemester-service-key") {
        return Err(Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")));
    }
    state
        .native
        .cookies
        .verify(one(headers, "cookie"), None)
        .map_err(|_| Box::new(refused(StatusCode::UNAUTHORIZED, "unauthorized")))
}
async fn companies(State(state): State<Product>, request: Request) -> Response {
    let cookie = match proof(&state, &request, false) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if request.uri().query().is_some() {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let native = state.native;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = native.owner.lock().map_err(|_| ())?;
        let catalog=owner.catalog().map_err(|_|())?;
        let result = owner
            .workspace_context(&native.cookies, &SystemClock, &cookie, &catalog)
            .map_err(|_| ())?;
        let context = result.outcome.map_err(|_| ())?;
        let mut value: Value = serde_json::from_str(context.as_json()).map_err(|_| ())?;
        // Profiles are read from each admitted ledger, under current company rights.
        let companies = value["companies"].as_array_mut().ok_or(())?;
        for company in companies {
            if company["archived"] == true { continue; }
            owner.select_company(company["slug"].as_str().ok_or(())?).map_err(|_|())?;
            let attempt = owner
                .company_setup(
                    &cookie,
                    &SystemClock,
                    rentemester_application::company_setup::Action::Profile,
                )
                .map_err(|_| ())?;
            let profile = attempt.outcome.map_err(|_| ())?.value;
            company["name"] = profile["name"].clone();
            company["createdAt"] = profile["createdAt"].clone();
        }
        Ok::<_, ()>(value)
    })
    .await;
    match result {
        Ok(Ok(value)) => response(
            StatusCode::OK,
            json!({"ok":true,"companies":value["companies"],"count":value["companies"].as_array().map(Vec::len)}),
        ),
        _ => refused(StatusCode::UNAUTHORIZED, "unauthorized"),
    }
}
#[derive(Default)]
struct Query {
    year: Option<String>,
    account: Option<String>,
    format: Option<String>,
}
fn query(request: &Request) -> Result<Query, Box<Response>> {
    let mut result = Query::default();
    for (key, value) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
    {
        let slot = match key.as_ref() {
            "year" => &mut result.year,
            "account" => &mut result.account,
            "format" => &mut result.format,
            _ => return Err(Box::new(refused(StatusCode::BAD_REQUEST, "invalid_query"))),
        };
        if slot.is_some() || value.len() > 128 {
            return Err(Box::new(refused(StatusCode::BAD_REQUEST, "invalid_query")));
        }
        *slot = Some(value.into_owned());
    }
    Ok(result)
}
fn selected(state:&Product,request:&Request)->bool {
    let slug=request.uri().path().split('/').nth(3).unwrap_or("");
    state.native.owner.lock().ok().and_then(|mut o|o.catalog().ok()).is_some_and(|c|c.companies.iter().any(|c|c.slug==slug&&!c.archived))
}

async fn read(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let query = match query(&request) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let exporting = request.uri().path().ends_with("/export");
    let resource = request
        .uri()
        .path()
        .rsplit('/')
        .nth(usize::from(exporting))
        .unwrap()
        .to_owned();
    let format = query.format.clone().unwrap_or_else(|| "csv".into());
    if (!exporting && query.format.is_some())
        || (exporting
            && !matches!(
                resource.as_str(),
                "journal" | "income-statement" | "balance" | "trial-balance"
            ))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_export");
    }
    let today = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
        .format("%Y-%m-%d")
        .to_string();
    let snapshot_today = today.clone();
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let native = state.native;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let today = snapshot_today;
        let mut owner = native.lock_company(&company_slug).map_err(|_| ())?;
        owner
            .cockpit_snapshot(
                &cookie,
                &SystemClock,
                CockpitRequest {
                    today: &today,
                    year: query.year.as_deref(),
                    account: query.account.as_deref(),
                },
            )
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match attempt.outcome {
            Ok(committed) => {
                let snapshot = committed.value;
                let (key, value) = match resource.as_str() {
                    "journal" => ("journal", snapshot.journal),
                    "trial-balance" => ("trialBalance", snapshot.trial_balance),
                    "income-statement" => ("incomeStatement", snapshot.income_statement),
                    "balance" => ("balance", snapshot.balance),
                    _ => ("fiscalYears", snapshot.fiscal_years),
                };
                if exporting {
                    let Some(export) =
                        crate::report_export::build(&resource, &format, &value, &today)
                    else {
                        return refused(StatusCode::BAD_REQUEST, "invalid_export");
                    };
                    let disposition = format!("attachment; filename*=UTF-8''{}", export.filename);
                    let mut reply = (StatusCode::OK, export.bytes).into_response();
                    reply.headers_mut().insert(
                        "content-type",
                        axum::http::HeaderValue::from_static(export.content_type),
                    );
                    reply.headers_mut().insert(
                        "content-disposition",
                        axum::http::HeaderValue::from_str(&disposition)
                            .expect("sanitized filename"),
                    );
                    reply.headers_mut().insert(
                        "cache-control",
                        axum::http::HeaderValue::from_static("private, no-store"),
                    );
                    reply
                } else {
                    response(StatusCode::OK, json!({"ok":true,key:value}))
                }
            }
            Err(f) if f.error.code == "invalid_credentials" => {
                refused(StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Err(f) if f.stage == FailureStage::NotCommitted => {
                refused(StatusCode::BAD_REQUEST, f.error.code)
            }
            _ => refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostWire<T> {
    confirm: bool,
    input: T,
}
enum Mutation {
    Post(journal_post::Input),
    Reverse(journal_reversal::Input),
}
type MutationResult = Result<
    Value,
    (
        rentemester_application::control_fence::Failure,
        Option<Value>,
    ),
>;
async fn write(State(state): State<Product>, request: Request) -> Response {
    let company_slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type").is_some_and(|s| {
            s.eq_ignore_ascii_case("application/json")
                || s.eq_ignore_ascii_case("application/json; charset=utf-8")
        })
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let reversal = request.uri().path().ends_with("/manual-journal-reversal");
    let bytes = match to_bytes(request.into_body(), 65_536).await {
        Ok(b) => b,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "body_too_large"),
    };
    let command = if reversal {
        let wire: PostWire<journal_reversal::Input> = match serde_json::from_slice(&bytes) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        if !wire.confirm || wire.input.idempotency_key.is_none() {
            return refused(StatusCode::BAD_REQUEST, "confirmation_and_key_required");
        }
        Mutation::Reverse(wire.input)
    } else {
        let wire: PostWire<journal_post::Input> = match serde_json::from_slice(&bytes) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        };
        if !wire.confirm || wire.input.idempotency_key.is_none() {
            return refused(StatusCode::BAD_REQUEST, "confirmation_and_key_required");
        }
        Mutation::Post(wire.input)
    };
    let native = state.native;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = native.lock_company(&company_slug).map_err(|_| ())?;
        let outcome: MutationResult = match command {
            Mutation::Post(input) => owner
                .post_human(
                    &cookie,
                    &SystemClock,
                    &journal_post::Policy::default(),
                    input,
                )
                .map_err(|_| ())?
                .outcome
                .map(|v| {
                    let mut value = v.value.result;
                    if let Some(receipt) = v.value.receipt {
                        value["receipt"] = json!(receipt);
                    }
                    value
                })
                .map_err(|f| (f.fence, f.business)),
            Mutation::Reverse(input) => owner
                .reverse_human(
                    &cookie,
                    &SystemClock,
                    &journal_reversal::Policy::default(),
                    input,
                )
                .map_err(|_| ())?
                .outcome
                .map(|v| {
                    let mut value = v.value.result;
                    if let Some(receipt) = v.value.receipt {
                        value["receipt"] = json!(receipt);
                    }
                    value
                })
                .map_err(|f| (f.fence, f.business)),
        };
        Ok::<_, ()>(outcome)
    })
    .await;
    match result {
        Ok(Ok(outcome)) => match outcome {
            Ok(value) => response(StatusCode::OK, value),
            Err((f, _)) if f.stage == FailureStage::OutcomeUnknown => {
                refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown")
            }
            Err((f, _)) if f.error.code == "invalid_credentials" => {
                refused(StatusCode::UNAUTHORIZED, "unauthorized")
            }
            Err((f, business)) => match business {
                Some(value) => response(StatusCode::UNPROCESSABLE_ENTITY, value),
                None => refused(StatusCode::BAD_REQUEST, f.error.code),
            },
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

mod ownership;

mod intercompany;
mod intercompany_dispositions;

mod eliminations;
mod consolidation;
