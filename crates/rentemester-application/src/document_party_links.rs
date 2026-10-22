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
    Internal(&'a store::Internal),
    InternalSupersede(&'a store::InternalSupersede),
    Asserted(&'a Action<'a>, Option<&'a str>, Option<&'a str>),
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

fn source_evidence(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
) -> Result<rentemester_storage::documents::Evidence, Error> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| store::refused("DOCUMENT_SOURCE_REFUSED"))?;
    if !exists {
        return Err(store::refused("DOCUMENT_NOT_FOUND"));
    }
    rentemester_storage::documents::evidence(tx, id)
}

impl<'a> Action<'a> {
    fn base(&self) -> &Action<'a> {
        match self {
            Self::Asserted(a, _, _) => a.base(),
            _ => self,
        }
    }
    fn write(&self) -> bool {
        matches!(
            self.base(),
            Self::Apply(_) | Self::Supersede(_) | Self::Internal(_) | Self::InternalSupersede(_)
        )
    }
    fn permission(&self) -> Permission {
        if self.write() {
            Permission::CompanyMasterData
        } else {
            Permission::CompanyDocumentsRead
        }
    }
    fn assertions(&self, actor: &str) -> Result<(), Error> {
        if let Self::Asserted(_, expected_actor, expected_principal) = self
            && (expected_actor.is_some_and(|s| s != actor)
                || expected_principal.is_some_and(|s| s != actor))
        {
            return Err(store::refused("access_denied"));
        }
        Ok(())
    }
    fn input(&self) -> Option<store::PlanInput> {
        match self.base() {
            Self::Plan(i) => Some((*i).clone()),
            Self::Apply(i) => Some(i.input()),
            _ => None,
        }
    }
    fn apply(
        &self,
        tx: &rusqlite::Transaction<'_>,
        guard: &mut NamespaceGuard,
        snapshot: Option<Result<Value, Error>>,
        actor: &str,
        now: &str,
    ) -> Result<Value, Error> {
        self.assertions(actor)?;
        store::validate_backup(tx)?;
        let id = match self.base() {
            Self::History(id) => Some(*id),
            Self::Plan(i) => Some(i.document_id),
            Self::Apply(i) => Some(i.document_id),
            Self::Supersede(i) => Some(i.document_id),
            Self::Internal(i) => Some(i.document_id),
            Self::InternalSupersede(i) => Some(i.document_id),
            _ => None,
        };
        if let Some(id) = id {
            let evidence = source_evidence(tx, id)?;
            guard
                .read_document(&evidence.path, &evidence.hash)
                .map_err(|_| store::refused("DOCUMENT_SOURCE_REFUSED"))?;
        }
        let result = match self.base() {
            Self::List(s, l) => store::list(tx, *s, *l),
            Self::History(id) => store::history(tx, *id),
            Self::Plan(i) => snapshot
                .ok_or_else(|| store::refused("PARTY_NOT_FOUND"))?
                .and_then(|p| store::plan(tx, i, &p)),
            Self::Apply(i) => store::apply(
                tx,
                i,
                &snapshot.ok_or_else(|| store::refused("PARTY_NOT_FOUND"))??,
                actor,
                actor,
                now,
            ),
            Self::Supersede(i) => store::supersede(tx, i, actor, actor, now),
            Self::Internal(i) => store::internal(tx, i, actor, actor, now),
            Self::InternalSupersede(i) => store::internal_supersede(tx, i, actor, actor, now),
            Self::Asserted(..) => unreachable!(),
        };
        match (result, self.base()) {
            (Err(e), Self::Plan(_)) => Ok(json!({"ok":false,"errors":[e.code]})),
            (r, _) => r,
        }
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let snapshot = RefCell::new(None);
    let write = action.write();
    control_fence::execute_authorized_with_backup(
        control,
        company,
        write.then_some(control_fence::BackupCheck {
            workspace: context.selection.workspace,
            slug: context.selection.slug,
            clock: context.clock,
        }),
        |tx| {
            let subject = authorize(tx, &context, write)?;
            action.assertions(&format!("user:{subject}"))?;
            if let Some(input) = action.input() {
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
            action.apply(
                tx,
                guard,
                snapshot.borrow_mut().take(),
                &format!("user:{subject}"),
                &now,
            )
        },
    )
}
/// Established attribution: both actor and principal are service_account:{current typed subject}.
/// The metered admission is reused; the subsequent control fence re-observes the exact live key,
/// grantor and scope while capturing the registry on that held writer.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, BusinessStage, OperationResult};
    let key = context.key;
    let clock = context.clock;
    let selection = control_fence::CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    };
    let permission = action.permission();
    let admitted = service_company::execute_operation(
        control,
        company,
        context,
        permission,
        action.write(),
        |_, subject, _| {
            action.assertions(&format!("service_account:{subject}"))?;
            Ok(OperationResult {
                result: subject.to_owned(),
                receipt: None,
            })
        },
    )?;
    // This is an admission, not the final document operation. Unusable cleanup
    // cannot authorize another transaction, even when metered admission committed.
    if !admitted.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(service_company::ServiceFailure {
            usage: admitted.usage,
            business: BusinessStage::NotCommitted,
            error: store::refused("owner_unavailable"),
            control_reusable: admitted.control_reusable && control.is_autocommit(),
            company_reusable: company.is_autocommit(),
            rejection: None,
        });
    }
    let catalog = rentemester_storage::workspace_context::Catalog {
        workspace: selection.workspace.into(),
        companies: vec![rentemester_storage::workspace_context::RegisteredCompany {
            slug: selection.slug.into(),
            name: selection.slug.into(),
            archived: selection.archived,
        }],
    };
    let snapshot = RefCell::new(None);
    let result = control_fence::execute_authorized_with_backup(
        control,
        company,
        action.write().then_some(control_fence::BackupCheck {
            workspace: selection.workspace,
            slug: selection.slug,
            clock,
        }),
        |tx| {
            if action.write() {
                rentemester_storage::workspace_transfer::require_business_write(tx)?;
            }
            let observed = rentemester_storage::service_context::observe(
                tx,
                key.lookup_hash(),
                clock
                    .now_unix_ms()
                    .map_err(|_| store::refused("CLOCK_UNAVAILABLE"))?,
                &catalog,
            )?;
            if observed.subject() != admitted.result {
                return Err(store::refused("invalid_credentials"));
            }
            let role = rentemester_storage::control_access::observe_company_role(
                tx,
                observed.subject(),
                selection.slug,
            )?
            .ok_or_else(|| store::refused("access_denied"))?;
            let role: CompanyRole =
                serde_json::from_value(json!(role)).map_err(|_| store::refused("access_denied"))?;
            if !access::company_role_allows(role, permission) {
                return Err(store::refused("access_denied"));
            }
            action.assertions(&format!("service_account:{}", observed.subject()))?;
            if let Some(input) = action.input() {
                *snapshot.borrow_mut() = Some(store::capture(tx, &input, selection.slug));
            }
            Ok(observed.subject().to_owned())
        },
        |tx, subject| {
            let now = rentemester_engine::company_knowledge::timestamp(
                clock
                    .now_unix_ms()
                    .map_err(|_| store::refused("CLOCK_UNAVAILABLE"))?,
            )?;
            action.apply(
                tx,
                guard,
                snapshot.borrow_mut().take(),
                &format!("service_account:{subject}"),
                &now,
            )
        },
    );
    match result {
        Ok(done) => Ok(service_company::ServiceResult {
            usage: admitted.usage,
            business: BusinessStage::Committed,
            result: done.value,
            receipt: None,
            control_reusable: admitted.control_reusable && done.control_reusable,
        }),
        Err(f) => Err(service_company::ServiceFailure {
            usage: admitted.usage,
            business: if f.stage == control_fence::FailureStage::OutcomeUnknown {
                BusinessStage::OutcomeUnknown
            } else {
                BusinessStage::NotCommitted
            },
            error: f.error,
            control_reusable: f.control_reusable,
            company_reusable: f.company_reusable,
            rejection: None,
        }),
    }
}
#[cfg(test)]
mod tests;
