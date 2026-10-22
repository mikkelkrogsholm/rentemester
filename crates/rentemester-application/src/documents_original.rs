//! Original API uses the existing current human/service writer fences.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, document::Metadata};
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub enum Action {
    List,
    Enrich(i64, Box<Metadata>),
    CompanyContext(i64, String, String),
}
impl Action {
    fn permission(&self) -> Permission {
        if matches!(self, Self::List) {
            Permission::CompanyDocumentsRead
        } else {
            Permission::CompanyDocumentsUpload
        }
    }
    fn apply(
        self,
        tx: &Transaction<'_>,
        guard: &mut NamespaceGuard,
        actor: &str,
    ) -> Result<Value, Error> {
        let id = match &self {
            Self::List => None,
            Self::Enrich(id, _) | Self::CompanyContext(id, _, _) => Some(*id),
        };
        if let Some(id) = id {
            let evidence = rentemester_storage::documents::evidence(tx, id)?;
            guard
                .read_document(&evidence.path, &evidence.hash)
                .map_err(|_| {
                    Error::new(
                        "DOCUMENT_ORIGINAL_REFUSED",
                        "original unavailable or integrity mismatch",
                    )
                })?;
        }
        match self {
            Self::List => rentemester_storage::documents_original::list(tx),
            Self::Enrich(id, metadata) => {
                rentemester_storage::documents_original::enrich(tx, id, &metadata, actor)
            }
            Self::CompanyContext(id, reference, reason) => {
                rentemester_storage::documents_original::company_context(
                    tx, id, &reference, &reason, actor,
                )
            }
        }
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        action.permission(),
        matches!(action, Action::List),
        |tx, subject| action.apply(tx, guard, &format!("user:{subject}")),
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    action: Action,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    service_company::execute_operation(
        control,
        company,
        context,
        action.permission(),
        !matches!(action, Action::List),
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: action.apply(tx, guard, &format!("service_account:{subject}"))?,
                receipt: None,
            })
        },
    )
}
