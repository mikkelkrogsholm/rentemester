//! Registry snapshot and company event share one current control fence and one company writer.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    control_fence::{self, Committed, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::document_party_links as store;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::cell::RefCell;
pub enum Action<'a> {
    List(Option<&'a str>, usize),
    History(i64),
    Plan(&'a store::PlanInput),
    Apply(&'a store::Apply),
    Supersede(&'a store::Supersede),
}
fn authorize(
    tx: &rusqlite::Transaction<'_>,
    context: &Context<'_>,
    write: bool,
) -> Result<String, Error> {
    let denied = || {
        Error::new(
            "access_denied",
            "current selected company document access or master-data permission required",
        )
    };
    if context.cookie.workspace() != context.selection.workspace
        || !context.selection.registered
        || (write && context.selection.archived)
    {
        return Err(denied());
    }
    if write {
        rentemester_storage::workspace_transfer::require_business_write(tx)?;
    }
    let now = context.clock.now_unix_ms().map_err(|_| denied())?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        context.cookie.token(),
        now,
        SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    let facts = rentemester_storage::control_access::observe_human(
        tx,
        &current.subject,
        &current.session_id,
        context.selection.slug,
    )?;
    let workspace: WorkspaceRole =
        serde_json::from_value(json!(facts.workspace_role.ok_or_else(denied)?))
            .map_err(|_| denied())?;
    let company: CompanyRole =
        serde_json::from_value(json!(facts.company_role.ok_or_else(denied)?))
            .map_err(|_| denied())?;
    if !access::workspace_role_allows(workspace, Permission::WorkspaceRead)
        || !access::company_role_allows(
            company,
            if write {
                Permission::CompanyMasterData
            } else {
                Permission::CompanyDocumentsRead
            },
        )
    {
        return Err(denied());
    }
    Ok(current.subject)
}

pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let snapshot = RefCell::new(None);
    let write = matches!(action, Action::Apply(_) | Action::Supersede(_));
    control_fence::execute_authorized_with_backup(
        control,
        company,
        write.then_some(control_fence::BackupCheck{workspace:context.selection.workspace,slug:context.selection.slug,clock:context.clock}),
        |tx| {
            let subject = authorize(tx, &context, write)?;
            let input = match action {
                Action::Plan(i) => Some(i.clone()),
                Action::Apply(i) => Some(i.input()),
                _ => None,
            };
            if let Some(input) = input {
                *snapshot.borrow_mut() = Some(store::capture(tx, &input, context.selection.slug));
            }
            Ok(subject)
        },
        |tx, subject| {
            let now = rentemester_engine::company_knowledge::timestamp(
                context
                    .clock
                    .now_unix_ms()
                    .map_err(|_| store::refused("CLOCK_UNAVAILABLE"))?,
            )?;
            let actor = format!("user:{subject}");
            let physical = |guard: &mut NamespaceGuard, id: i64| -> Result<(), Error> {
                let evidence = rentemester_storage::documents::evidence(tx, id)?;
                guard
                    .read_document(&evidence.path, &evidence.hash)
                    .map_err(|_| store::refused("DOCUMENT_SOURCE_REFUSED"))?;
                Ok(())
            };
            let result = match action {
                Action::List(status, limit) => store::list(tx, status, limit),
                Action::History(id) => store::history(tx, id),
                Action::Plan(input) => {
                    let captured = snapshot
                        .borrow_mut()
                        .take()
                        .ok_or_else(|| store::refused("PARTY_NOT_FOUND"))?;
                    match captured {
                        Ok(p) => store::plan(tx, input, &p),
                        Err(e) => Err(e),
                    }
                }
                Action::Apply(input) => {
                    let p = snapshot
                        .borrow_mut()
                        .take()
                        .ok_or_else(|| store::refused("PARTY_NOT_FOUND"))??;
                    physical(guard, input.document_id)?;
                    store::apply(tx, input, &p, &actor, &actor, &now)
                }
                Action::Supersede(input) => {
                    physical(guard, input.document_id)?;
                    store::supersede(tx, input, &actor, &actor, &now)
                }
            };
            match (result, &action) {
                (Err(e), Action::Plan(_)) => Ok(json!({"ok":false,"errors":[e.code]})),
                (r, _) => r,
            }
        },
    )
}

#[cfg(test)]
mod tests;
