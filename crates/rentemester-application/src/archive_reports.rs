//! Ordinary source-selected company reads on one current-authority/physical-source barrier.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::archive_reports::{self as store, Resource};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub struct Request<'a> {
    pub today: &'a str,
    pub year: Option<&'a str>,
    pub source: Option<&'a str>,
    pub account: Option<&'a str>,
    pub as_of: Option<&'a str>,
    pub resource: Resource,
}
pub enum Action<'a> {
    Read(Request<'a>),
    DatedReport(rentemester_storage::dated_reports::Input),
    JournalHistory(rentemester_storage::journal_history::Query),
    FiscalYears { today: &'a str },
}
fn invalid() -> Error {
    Error::new(
        "invalid_report_query",
        "Unknown or incompatible report selectors",
    )
}
fn unavailable() -> Error {
    Error::new(
        "report_source_unavailable",
        "Current report sources unavailable or integrity differs",
    )
}
fn inventory(
    tx: &Transaction<'_>,
    verified: &[rentemester_storage::archive_import::VerifiedArchive],
    today: &str,
) -> Result<Value, Error> {
    let mut years = rentemester_storage::cockpit::live_years(tx, today)?;
    let entries = years.as_array_mut().ok_or_else(unavailable)?;
    for year in rentemester_storage::archive_import::read(tx, verified, None)?["years"]
        .as_array()
        .ok_or_else(unavailable)?
    {
        let label = format!(
            "{:04}",
            year["fiscalYear"].as_u64().ok_or_else(unavailable)?
        );
        entries.push(
            json!({"label":label,"start":null,"end":null,"source":"archive","selectable":true}),
        );
    }
    entries.sort_by(|a, b| {
        b["label"]
            .as_str()
            .cmp(&a["label"].as_str())
            .then_with(|| a["source"].as_str().cmp(&b["source"].as_str()))
    });
    Ok(years)
}
fn select<'a>(years: &'a Value, request: &Request<'_>) -> Result<&'a Value, Error> {
    if request
        .source
        .is_some_and(|s| !matches!(s, "live" | "archive"))
        || request.source == Some("archive") && request.year.is_none()
    {
        return Err(invalid());
    }
    if request.account.is_some_and(|s| {
        s.trim() != s || s.is_empty() || s.len() > 128 || s.chars().any(char::is_control)
    }) || request.account.is_some() && !matches!(request.resource, Resource::Journal)
    {
        return Err(invalid());
    }
    if request.as_of.is_some() && !matches!(request.resource, Resource::Overview) {
        return Err(invalid());
    }
    if let Some(as_of) = request.as_of {
        rentemester_engine::fiscal_year::Date::parse(as_of).map_err(|_| invalid())?;
    }
    let rows = years.as_array().ok_or_else(unavailable)?;
    let year = if let Some(year) = request.year {
        if year.is_empty()
            || year.len() > 128
            || year.trim() != year
            || year.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        year
    } else {
        rows.iter()
            .find(|y| {
                y["source"] == "live"
                    && y["start"].as_str().is_some_and(|s| s <= request.today)
                    && y["end"].as_str().is_some_and(|s| s >= request.today)
            })
            .and_then(|y| y["label"].as_str())
            .ok_or_else(unavailable)?
    };
    rows.iter()
        .find(|y| y["label"] == year && y["source"] == request.source.unwrap_or("live"))
        .or_else(|| {
            if request.source.is_none() {
                rows.iter()
                    .find(|y| y["label"] == year && y["source"] == "archive")
            } else {
                None
            }
        })
        .ok_or_else(|| Error::new("invalid_year", "Requested source/year is not available"))
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        |tx, _| read_authorized(tx, guard, slug, action),
    )
}

pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult, crate::service_company::ServiceFailure> {
    let slug = context.selection.slug;
    crate::service_company::execute_read(control, company, context, |tx| {
        read_authorized(tx, guard, slug, action)
    })
}

/// Projection shared only by adapters that already hold current company authority.
pub(crate) fn read_authorized(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    slug: &str,
    action: Action<'_>,
) -> Result<Value, Error> {
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(unavailable());
    }
    let verified = crate::archive_import::verify_physical(tx, guard)?;
    if let Action::DatedReport(input) = &action {
        return rentemester_storage::dated_reports::read(tx, input);
    }
    if let Action::JournalHistory(query) = &action {
        return rentemester_storage::journal_history::read(tx, query);
    }
    let today = match &action {
        Action::Read(request) => request.today,
        Action::FiscalYears { today } => today,
        Action::JournalHistory(_) | Action::DatedReport(_) => unreachable!(),
    };
    let years = inventory(tx, &verified, today)?;
    let Action::Read(request) = action else {
        return Ok(json!({"slug":slug,"years":years}));
    };
    let selected = select(&years, &request)?;
    let label = selected["label"].as_str().ok_or_else(unavailable)?;
    let archived = selected["source"] == "archive";
    let mut body = if archived {
        if request.as_of.is_some() {
            return Err(Error::new(
                "archive_date_report_unsupported",
                "Archive closing dates are unknown; arbitrary-date reports require separate source assurance",
            ));
        }
        if label.len() != 4 || !label.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let year = label.parse::<u16>().map_err(|_| invalid())?;
        store::read(tx, &verified, slug, year, request.account, request.resource)?
    } else {
        let start = selected["start"].as_str().ok_or_else(unavailable)?;
        let end = selected["end"].as_str().ok_or_else(unavailable)?;
        let as_of = if matches!(request.resource, Resource::Overview) {
            request.as_of.unwrap_or(today)
        } else {
            end
        };
        let basis = if rentemester_storage::opening_balance::validate_history(tx)?.is_some() {
            rentemester_storage::opening_balance::require_report_as_of(tx, as_of)?
        } else {
            json!({"basis":"live","cutOverDate":null,"openingJournalEntryId":null,"archiveAmountsIncluded":false})
        };
        let cutover = basis["cutOverDate"].as_str();
        let partial = cutover.is_some_and(|date| start < date);
        let mut body = match request.resource {
            Resource::Overview => {
                rentemester_storage::dashboard::read(tx, slug, today, Some(label), request.as_of)?
            }
            resource => {
                let snapshot = rentemester_storage::cockpit::read(
                    tx,
                    slug,
                    today,
                    Some(label),
                    request.account,
                )?;
                match resource {
                    Resource::Journal => snapshot.journal,
                    Resource::TrialBalance => snapshot.trial_balance,
                    Resource::IncomeStatement => snapshot.income_statement,
                    Resource::Balance => snapshot.balance,
                    Resource::Overview => unreachable!(),
                }
            }
        };
        body["source"] = json!("live");
        body["selectedSource"] = json!("live");
        body["ledgerBasis"] = json!("native_ledger_only");
        body["openingBalanceBasis"] = basis.clone();
        body["liveCoverage"] = json!({"status":if partial{"partial"}else{"complete"},
                    "reason":if partial{Some("native_history_begins_at_explicit_cutover")}else{None},
                    "periodStart":start,"periodEnd":end,"cutOverDate":basis["cutOverDate"]});
        if years
            .as_array()
            .ok_or_else(unavailable)?
            .iter()
            .any(|y| y["source"] == "archive")
        {
            body["limitations"] = json!([
                "Native ledger only; archived closing balances are not opening balances or carry-forward entries."
            ]);
        }
        body
    };
    body["fiscalYears"] = years;
    Ok(body)
}
