//! Caller-driven scheduled generation under the ordinary current-session commit fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::scheduled_recurring as engine;
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::{
    recurring_invoices::{self as recurring, Prepared},
    scheduled_recurring as storage,
};
use rusqlite::Connection;
use serde_json::{Value, json};
pub enum Action<'a> {
    List,
    Plan(i64),
    Enable(&'a engine::Enable),
    Pause(&'a engine::Change),
    Resume(&'a engine::Change),
    Cancel(&'a engine::Change),
    CatchUp(&'a engine::Selection),
    Skip(&'a engine::Skip),
    Dispatch(&'a engine::Dispatch),
    Reconsent(&'a engine::Reconsent),
}
#[allow(clippy::too_many_arguments)]
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
    now: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        if matches!(action, Action::List | Action::Plan(_)) {
            Permission::CompanyRead
        } else if matches!(action, Action::Reconsent(_)) {
            Permission::CompanyAdmin
        } else {
            Permission::CompanyDraftWrite
        },
        |tx, actor| match action {
            Action::List => Ok(json!({"scheduledRecurring":storage::list(tx,slug,today)?})),
            Action::Plan(id) => Ok(json!({"plan":storage::plan(tx,id,today)?})),
            Action::Reconsent(i) => {
                Ok(json!({"scheduledRecurring":storage::reconsent(tx,i,actor,today,now)?}))
            }
            Action::Enable(i) => {
                Ok(json!({"scheduledRecurring":storage::enable(tx,i,actor,today,now)?}))
            }
            Action::Pause(i) | Action::Resume(i) | Action::Cancel(i) => {
                let kind = match action {
                    Action::Pause(_) => "paused",
                    Action::Resume(_) => "resumed",
                    _ => "cancelled",
                };
                Ok(json!({"scheduledRecurring":storage::change(tx,kind,i,actor,today,now)?}))
            }
            Action::CatchUp(i) => Ok(
                json!({"scheduledRecurring":storage::select(tx,i.template_id,&i.plan_hash,&i.period_indexes,None,i.confirm,actor,today,now)?}),
            ),
            Action::Skip(i) => Ok(
                json!({"scheduledRecurring":storage::select(tx,i.template_id,&i.plan_hash,&i.period_indexes,Some(&i.reason),i.confirm,actor,today,now)?}),
            ),
            Action::Dispatch(i) => {
                let occurrence = storage::begin(tx, i, actor, today, now)?;
                let result = issue(
                    tx,
                    guard,
                    i.template_id,
                    &occurrence,
                    actor,
                    today,
                    now,
                    rentemester_storage::idempotency::PrincipalKind::User,
                )?;
                storage::finish(tx, i.template_id, occurrence, result, actor, today, now)
            }
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn issue(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
    occurrence: &engine::Occurrence,
    actor: &str,
    today: &str,
    now: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Result<Value, String>, rentemester_engine::Error> {
    tx.execute_batch("SAVEPOINT scheduled_invoice_issue")
        .map_err(|_| engine::refused())?;
    let result = (|| {
        let request = recurring::Generate {
            as_of_date: occurrence.issue_date.clone(),
            confirm: true,
        };
        match recurring::prepare(tx, id, &request, today, now)? {
            Prepared::Existing(v) => {
                crate::sales::verify_invoice(
                    tx,
                    guard,
                    v["documentId"].as_i64().ok_or_else(engine::refused)?,
                )?;
                Ok(v)
            }
            Prepared::Issue(p) => {
                let issued = crate::sales::issue_payload_with_principal(
                    tx, guard, &p.payload, actor, principal,
                )?;
                recurring::finish_with_principal(tx, &p, &issued, actor, principal)
            }
        }
    })();
    let result = match result {
        Ok(v) => {
            tx.execute_batch("RELEASE scheduled_invoice_issue")
                .map_err(|_| engine::refused())?;
            Ok(v)
        }
        Err(e) => {
            tx.execute_batch(
                "ROLLBACK TO scheduled_invoice_issue; RELEASE scheduled_invoice_issue",
            )
            .map_err(|_| engine::refused())?;
            Err(e.code.to_owned())
        }
    };
    Ok(result)
}
