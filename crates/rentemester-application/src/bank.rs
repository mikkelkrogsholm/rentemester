//! Bank browser use cases under the current-session company fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
pub use rentemester_storage::bank::AccountInput;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Accounts,
    Read {
        today: &'a str,
        year: Option<&'a str>,
    },
    Create(&'a AccountInput),
    ImportBytes {
        content: &'a [u8],
        account: Option<&'a str>,
        profile: Option<&'a str>,
        now: &'a str,
    },
    Import {
        content: &'a str,
        account: Option<&'a str>,
        profile: Option<&'a str>,
        now: &'a str,
    },
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Accounts | Action::Read { .. } => Permission::CompanyRead,
        Action::Create(_) => Permission::CompanyAdmin,
        Action::Import { .. } | Action::ImportBytes { .. } => Permission::CompanyLedgerPost,
    };
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
        permission,
        |tx, subject| match action {
            Action::Accounts => rentemester_storage::bank::accounts(tx, slug),
            Action::Read { today, year } => rentemester_storage::bank::read(tx, slug, today, year),
            Action::Create(input) => rentemester_storage::bank::create_account(tx, input, subject),
            Action::Import {
                content,
                account,
                profile,
                now,
            } => import(
                tx,
                guard,
                content.as_bytes(),
                account,
                profile,
                subject,
                now,
            ),
            Action::ImportBytes {
                content,
                account,
                profile,
                now,
            } => import(tx, guard, content, account, profile, subject, now),
        },
    )
}

fn import(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    bytes: &[u8],
    account: Option<&str>,
    profile: Option<&str>,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    import_with_principal(
        tx,
        guard,
        bytes,
        account,
        profile,
        actor,
        now,
        rentemester_storage::idempotency::PrincipalKind::User,
    )
}
#[allow(clippy::too_many_arguments)]
fn import_with_principal(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    bytes: &[u8],
    account: Option<&str>,
    profile: Option<&str>,
    actor: &str,
    now: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    let content = rentemester_engine::bank::decode_bytes(bytes, profile)?;
    let rows = rentemester_engine::bank::parse_with_profile(&content, profile)?;
    rentemester_storage::bank::validate_import_account(tx, account)?;
    let archive = guard.publish_bank_statement_bytes(bytes).map_err(|_| {
        Error::new(
            "bank_original_unavailable",
            "bank statement original could not be preserved",
        )
    })?;
    rentemester_storage::bank::import_bytes_with_principal(
        tx, bytes, &rows, account, actor, now, &archive, principal,
    )
}

/// Deliberate service banking slice; account administration and text import stay human-only.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, OperationResult};
    let (permission, mutation) = match action {
        Action::Accounts | Action::Read { .. } => (Permission::CompanyRead, false),
        Action::ImportBytes { .. } => (Permission::CompanyLedgerPost, true),
        _ => {
            return Err(service_company::initial(Error::new(
                "access_denied",
                "service bank action not supported",
            )));
        }
    };
    let slug = context.selection.slug;
    service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            let result = match action {
                Action::Accounts => rentemester_storage::bank::accounts(tx, slug)?,
                Action::Read { today, year } => {
                    rentemester_storage::bank::read(tx, slug, today, year)?
                }
                Action::ImportBytes {
                    content,
                    account,
                    profile,
                    now,
                } => import_with_principal(
                    tx,
                    guard,
                    content,
                    account,
                    profile,
                    subject,
                    now,
                    rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                )?,
                _ => unreachable!("service action admitted above"),
            };
            Ok(OperationResult {
                result,
                receipt: None,
            })
        },
    )
}
