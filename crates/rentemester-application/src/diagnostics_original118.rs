//! Original diagnostic projections. Business dates never participate in authentication.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    service_key::ServiceKey,
};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{Value, json};

fn retention(tx: &Transaction<'_>, slug: &str, date: &str) -> Result<Value, Error> {
    if rentemester_engine::fiscal_year::Date::parse(date).is_err() {
        return Ok(
            json!({"ok":false,"asOf":date,"appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"rows":[],"errors":["asOf must be YYYY-MM-DD"]}),
        );
    }
    let source = rentemester_storage::retention::read(tx, slug, date)?;
    let mut report = source["report"].clone();
    // The native read contains additional administrative metadata, not original CORE.
    for row in report["rows"]
        .as_array_mut()
        .ok_or_else(|| Error::new("invalid_report", "Retention report unavailable"))?
    {
        row.as_object_mut()
            .ok_or_else(|| Error::new("invalid_report", "Retention row unavailable"))?
            .retain(|k, _| {
                matches!(
                    k.as_str(),
                    "table" | "total" | "expired" | "nextExpiry" | "oldestExpired"
                )
            });
    }
    Ok(report)
}
pub fn retention_human(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    date: &str,
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
        |tx, _| retention(tx, slug, date),
    )
}
pub fn retention_service(
    control: &mut Connection,
    company: &mut Connection,
    context: service_company::Context<'_>,
    date: &str,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let slug = context.selection.slug;
    service_company::execute_read(control, company, context, |tx| retention(tx, slug, date))
}
#[derive(Clone, Copy)]
pub enum Credential<'a> {
    Human(&'a CookieVerifier, &'a VerifiedCookie),
    Service(&'a ServiceKey),
}
/// Control-only health admission: callback receives ONLY a validated registered slug.
/// The host owns the protected physical root; this action never opens a target ledger.
pub fn health(
    control: &mut Connection,
    clock: &dyn Clock,
    credential: Credential<'_>,
    catalog: &Catalog,
    slug: &str,
    inspect: impl FnOnce(&str) -> Value,
) -> Result<Value, crate::password_login::Failure> {
    use crate::mfa_login::{error, failure};
    control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
    let workspace = match credential {
        Credential::Human(c, p) if c.accepts_session(p) => p.workspace(),
        Credential::Service(k) => k.workspace(),
        _ => return Err(failure(error("invalid_credentials"), true)),
    };
    if workspace != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let mut charged = false;
    let result = (|| {
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let (subject, workspace_role, observed) = match credential {
            Credential::Human(_, proof) => {
                let current = rentemester_storage::cookie_session::resolve(
                    &tx,
                    proof.token(),
                    now,
                    rentemester_storage::cookie_session::SessionRequirement::Current,
                )?;
                rentemester_storage::session_control::authorize(&tx, &current)?;
                let facts = rentemester_storage::control_access::observe_human(
                    &tx,
                    &current.subject,
                    &current.session_id,
                    slug,
                )?;
                (
                    current.subject,
                    facts.workspace_role.ok_or_else(|| error("access_denied"))?,
                    None,
                )
            }
            Credential::Service(key) => {
                let observed = rentemester_storage::service_context::observe(
                    &tx,
                    key.lookup_hash(),
                    now,
                    catalog,
                )?;
                (
                    observed.subject().to_owned(),
                    observed.workspace_role().to_owned(),
                    Some(observed),
                )
            }
        };
        rentemester_storage::workspace_members::company(catalog, slug, true)?;
        let wr: WorkspaceRole =
            serde_json::from_value(json!(workspace_role)).map_err(|_| error("invalid_control"))?;
        let cr: CompanyRole = serde_json::from_value(json!(
            rentemester_storage::control_access::observe_company_role(&tx, &subject, slug)?
                .ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        if !access::workspace_role_allows(wr, Permission::WorkspaceRead)
            || !access::company_role_allows(cr, Permission::CompanyRead)
        {
            return Err(error("access_denied"));
        }
        if let Credential::Service(key) = credential {
            if rentemester_storage::workspace_transfer::current(&tx)?.mode == "active" {
                let charge = rentemester_storage::service_context::charge(
                    &tx,
                    key.lookup_hash(),
                    now,
                    catalog,
                )?;
                if !charge.same_credential(
                    observed
                        .as_ref()
                        .ok_or_else(|| error("invalid_credentials"))?,
                ) {
                    return Err(error("invalid_credentials"));
                }
                charged = true;
            }
        }
        Ok(inspect(slug))
    })();
    match result {
        Ok(value) => {
            if charged {
                tx.commit().map_err(|_| crate::password_login::Failure {
                    stage: crate::password_login::FailureStage::OutcomeUnknown,
                    error: error("commit_outcome_unknown"),
                    connection_reusable: false,
                })?;
            } else {
                tx.rollback()
                    .map_err(|_| failure(error("connection_refused"), false))?;
            }
            control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
            Ok(value)
        }
        Err(e) => {
            let reusable =
                tx.rollback().is_ok() && control_fence::connection_ready(control).is_ok();
            Err(failure(e, reusable))
        }
    }
}
#[cfg(test)]
#[path = "diagnostics_original118_tests.rs"]
mod tests;
