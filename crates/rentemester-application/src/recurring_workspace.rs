//! Phased explicit recurring runner; transport remains outside the held company transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::{recurring_invoices::original, recurring_workspace as store};
use rusqlite::Connection;
use serde_json::Value;
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Plan {
        as_of: &'a str,
        max: usize,
    },
    Generate {
        template_id: i64,
        period_index: u32,
        as_of: &'a str,
    },
    Reserve {
        generation_id: i64,
    },
    Event {
        generation_id: i64,
        event_type: &'a str,
        provider_id: Option<&'a str>,
        message: &'a str,
    },
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
    now: &str,
) -> Result<Committed<Value>, Failure> {
    let public = match action {
        Action::Reserve { generation_id } | Action::Event { generation_id, .. } => {
            store::channel(company, generation_id).is_ok_and(|s| s == "digisense")
        }
        _ => false,
    };
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: if public && matches!(action, Action::Reserve { .. }) {
                SessionRequirement::Fresh
            } else {
                SessionRequirement::Current
            },
        },
        context.selection,
        if public {
            Permission::CompanyExternalSend
        } else {
            Permission::CompanyDraftWrite
        },
        |tx, actor| match action {
            Action::Plan { as_of, max } => {
                if rentemester_engine::fiscal_year::Date::parse(as_of)?
                    > rentemester_engine::fiscal_year::Date::parse(today)?
                {
                    return Err(rentemester_engine::Error::new(
                        "recurring_workspace_refused",
                        "asOfDate no later than today required",
                    ));
                }
                let plan = store::plan(tx, as_of, max)?;
                if let Some(rows) = plan["deliveries"].as_array() {
                    for row in rows {
                        crate::sales::verify_invoice(
                            tx,
                            guard,
                            row["documentId"].as_i64().ok_or_else(|| {
                                rentemester_engine::Error::new(
                                    "recurring_workspace_refused",
                                    "delivery document identity unavailable",
                                )
                            })?,
                        )?;
                    }
                }
                Ok(plan)
            }
            Action::Generate {
                template_id,
                period_index,
                as_of,
            } => {
                store::validate_history(tx)?;
                let input = original::Generate {
                    template_id,
                    as_of_date: as_of.to_owned(),
                    confirm: true,
                };
                let prepared =
                    original::prepare_period(tx, &input, today, now, Some(period_index))?;
                crate::recurring_invoices::original_prepared(
                    tx,
                    guard,
                    prepared,
                    actor,
                    rentemester_storage::idempotency::PrincipalKind::User,
                )
            }
            Action::Reserve { generation_id } => {
                crate::sales::verify_invoice(tx, guard, store::document_id(tx, generation_id)?)?;
                store::reserve(tx, generation_id, actor)
            }
            Action::Event {
                generation_id,
                event_type,
                provider_id,
                message,
            } => {
                crate::sales::verify_invoice(tx, guard, store::document_id(tx, generation_id)?)?;
                store::event(tx, generation_id, event_type, provider_id, message, actor)
            }
        },
    )
}
