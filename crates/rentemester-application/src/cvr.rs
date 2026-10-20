//! Current authorization on both sides of provider I/O, with no SQL transaction held across it.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::{contact_import::Lookup, namespace::NamespaceGuard};
use rentemester_storage::{contact_import::LookupOutcome, cvr as store};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Lookup(&'a str),
    Sync,
}
fn fail(code: &'static str, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error: Error::new(code, "Native CVR operation refused before publication"),
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn auth<'a>(context: &Context<'a>) -> CookieAuthorization<'a> {
    CookieAuthorization {
        cookie: context.cookie,
        clock: context.clock,
        requirement: SessionRequirement::Current,
    }
}
fn selection<'a>(context: &Context<'a>) -> CompanySelection<'a> {
    CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    }
}
fn now(context: &Context<'_>) -> Result<String, Failure> {
    rentemester_engine::company_knowledge::timestamp(
        context
            .clock
            .now_unix_ms()
            .map_err(|_| fail("invalid_auth_clock", true))?,
    )
    .map_err(|_| fail("invalid_auth_clock", true))
}
struct Prepared {
    input: store::Prepared,
    subject: String,
    ledger: String,
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    lookup: &mut dyn Lookup,
) -> Result<Committed<Value>, Failure> {
    let sync = matches!(action, Action::Sync);
    let stamp = now(&context)?;
    let prepared = control_fence::execute_cookie_subject(
        control,
        company,
        auth(&context),
        selection(&context),
        Permission::CompanyExternalLookup,
        |tx, subject| {
            let changes = tx.total_changes();
            let input = store::prepare(
                tx,
                match action {
                    Action::Lookup(cvr) => Some(cvr),
                    Action::Sync => None,
                },
                sync,
                &stamp,
            )?;
            let ledger = tx
                .query_row(
                    "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    Error::new(
                        "cvr_source_unavailable",
                        "Native ledger identity unavailable",
                    )
                })?;
            if tx.total_changes() != changes {
                return Err(Error::new(
                    "readonly_operation_changed_data",
                    "CVR prepare must not mutate",
                ));
            }
            Ok(Prepared {
                input,
                subject: subject.into(),
                ledger,
            })
        },
    )?;
    if !prepared.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(fail("cvr_connection_unavailable", false));
    }
    let prepared = prepared.value;
    let outcome = if prepared.input.needs_lookup {
        Some(
            match lookup.lookup(prepared.input.cvr.as_deref().unwrap()) {
                Ok(entity)
                    if serde_json::to_vec(&entity)
                        .is_ok_and(|bytes| bytes.len() <= 4 * 1024 * 1024) =>
                {
                    LookupOutcome::Found(entity)
                }
                Ok(_) => {
                    LookupOutcome::Failed("CVR returnerede et ugyldigt eller for stort svar".into())
                }
                Err(error) => LookupOutcome::Failed(error.message().into()),
            },
        )
    } else {
        None
    };
    guard
        .ensure_connections(control, company)
        .map_err(|_| fail("cvr_namespace_changed", false))?;
    let stamp = now(&context)?;
    // Current policy grants ExternalLookup and MasterData to the same company
    // owner/bookkeeper roles. Sync additionally checks MasterData at final apply;
    // the first fence already denied all other roles before cache/provider access.
    control_fence::execute_cookie_subject(
        control,
        company,
        auth(&context),
        selection(&context),
        if sync {
            Permission::CompanyMasterData
        } else {
            Permission::CompanyExternalLookup
        },
        |tx, subject| {
            let ledger: String = tx
                .query_row(
                    "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    Error::new(
                        "cvr_source_unavailable",
                        "Native ledger identity unavailable",
                    )
                })?;
            if subject != prepared.subject || ledger != prepared.ledger {
                return Err(Error::new(
                    "cvr_identity_changed",
                    "CVR subject or ledger changed during lookup",
                ));
            }
            store::finish(tx, &prepared.input, outcome.as_ref(), subject, &stamp)
        },
    )
}
#[cfg(test)]
mod tests;
