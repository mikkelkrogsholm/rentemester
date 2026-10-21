//! Bank browser use cases under the current-session company fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
pub use rentemester_storage::bank::AccountInput;
pub use rentemester_storage::bank_accounts_original::Patch as AccountPatch;
pub use rentemester_storage::bank_queries::{AccountsQuery, ListQuery, ReportQuery};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Original(rentemester_storage::bank_original::Action<'a>),
    Accounts,
    Transactions(&'a ListQuery),
    Reconciliation(&'a ReportQuery),
    AccountList(&'a AccountsQuery),
    Read {
        today: &'a str,
        year: Option<&'a str>,
    },
    Create(&'a AccountInput),
    CreateOriginal(&'a AccountInput),
    UpdateOriginal {
        account: &'a str,
        patch: &'a AccountPatch,
    },
    UpdateBrowser {
        account: &'a str,
        patch: &'a AccountPatch,
    },
    ImportOriginal {
        content: &'a [u8],
        account: Option<&'a str>,
        profile: Option<&'a str>,
        now: &'a str,
    },
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
        Action::Original(command) => crate::bank_original::permission(command),
        Action::Accounts
        | Action::Read { .. }
        | Action::Transactions(_)
        | Action::Reconciliation(_)
        | Action::AccountList(_) => Permission::CompanyRead,
        Action::Create(_) | Action::UpdateBrowser { .. } => Permission::CompanyAdmin,
        Action::CreateOriginal(_) | Action::UpdateOriginal { .. } => Permission::CompanyMasterData,
        Action::Import { .. } | Action::ImportBytes { .. } | Action::ImportOriginal { .. } => {
            Permission::CompanyLedgerPost
        }
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
            Action::Original(command) => crate::bank_original::read_authorized(
                tx,
                guard,
                command,
                subject,
                rentemester_storage::idempotency::PrincipalKind::User,
                context.clock,
            ),
            Action::Transactions(_) | Action::Reconciliation(_) | Action::AccountList(_) => {
                query_authorized(tx, guard, &action)
            }
            Action::Accounts => rentemester_storage::bank::accounts(tx, slug),
            Action::Read { today, year } => rentemester_storage::bank::read(tx, slug, today, year),
            Action::CreateOriginal(input) => rentemester_storage::bank_accounts_original::create(
                tx,
                input,
                subject,
                rentemester_storage::idempotency::PrincipalKind::User,
            )
            .map(|o| o.original()),
            Action::UpdateOriginal { account, patch } => {
                rentemester_storage::bank_accounts_original::update(
                    tx,
                    account,
                    patch,
                    subject,
                    rentemester_storage::idempotency::PrincipalKind::User,
                )
                .map(|o| o.original())
            }
            Action::UpdateBrowser { account, patch } => {
                rentemester_storage::bank_accounts_original::update(
                    tx,
                    account,
                    patch,
                    subject,
                    rentemester_storage::idempotency::PrincipalKind::User,
                )?
                .browser()
            }
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
            Action::ImportOriginal {
                content,
                account,
                profile,
                now,
            } => import_original(
                tx,
                guard,
                content,
                account,
                profile,
                subject,
                now,
                rentemester_storage::idempotency::PrincipalKind::User,
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

/// Deliberate service banking slice; original master data uses its explicit right.
/// Native administration and text import stay human-only.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    use crate::service_company::{self, OperationResult};
    let clock = context.clock;
    let (permission, mutation) = match action {
        Action::Original(command) => (
            crate::bank_original::permission(command),
            crate::bank_original::mutation(command),
        ),
        Action::Accounts
        | Action::Read { .. }
        | Action::Transactions(_)
        | Action::Reconciliation(_)
        | Action::AccountList(_) => (Permission::CompanyRead, false),
        Action::CreateOriginal(_) | Action::UpdateOriginal { .. } => {
            (Permission::CompanyMasterData, true)
        }
        Action::ImportBytes { .. } | Action::ImportOriginal { .. } => {
            (Permission::CompanyLedgerPost, true)
        }
        _ => {
            // This branch has not observed or touched either connection. Do not
            // retire the owner or imply an unknown outcome for an unsupported action.
            let mut failure = service_company::initial(Error::new(
                "access_denied",
                "service bank action not supported",
            ));
            failure.control_reusable = true;
            failure.company_reusable = true;
            return Err(failure);
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
                Action::Original(command) => crate::bank_original::read_authorized(
                    tx,
                    guard,
                    command,
                    subject,
                    rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                    clock,
                )?,
                Action::Transactions(_) | Action::Reconciliation(_) | Action::AccountList(_) => {
                    query_authorized(tx, guard, &action)?
                }
                Action::Accounts => rentemester_storage::bank::accounts(tx, slug)?,
                Action::Read { today, year } => {
                    rentemester_storage::bank::read(tx, slug, today, year)?
                }
                Action::CreateOriginal(input) => {
                    rentemester_storage::bank_accounts_original::create(
                        tx,
                        input,
                        subject,
                        rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                    )?
                    .original()
                }
                Action::UpdateOriginal { account, patch } => {
                    rentemester_storage::bank_accounts_original::update(
                        tx,
                        account,
                        patch,
                        subject,
                        rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                    )?
                    .original()
                }
                Action::ImportOriginal {
                    content,
                    account,
                    profile,
                    now,
                } => import_original(
                    tx,
                    guard,
                    content,
                    account,
                    profile,
                    subject,
                    now,
                    rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
                )?,
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

/// Current authority and both database transactions are already held by the caller.
fn query_authorized(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    action: &Action<'_>,
) -> Result<Value, Error> {
    verify_query_sources(tx, guard)?;
    match action {
        Action::Transactions(query) => rentemester_storage::bank_queries::list(tx, query),
        Action::Reconciliation(query) => rentemester_storage::bank_queries::report(tx, query),
        Action::AccountList(query) => rentemester_storage::bank_queries::accounts(tx, query),
        _ => unreachable!("only original bank query actions admitted"),
    }
}

pub(crate) fn verify_query_sources(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
) -> Result<(), Error> {
    let unavailable = || {
        Error::new(
            "bank_query_sources_unavailable",
            "Current bank report sources unavailable or integrity differs",
        )
    };
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(unavailable());
    }
    crate::archive_import::verify_physical(tx, guard)?;
    // Ordinary period controls cover referenced bank rows. Lists also expose
    // unmatched imports, whose exact original and captured row must be proved.
    let mut q = tx
        .prepare("SELECT id,source_file_hash FROM bank_transactions ORDER BY id")
        .map_err(|_| unavailable())?;
    let sources = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|_| unavailable())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| unavailable())?;
    for (id, hash) in sources {
        let bytes = guard
            .read_bank_statement(&hash)
            .map_err(|_| unavailable())?;
        rentemester_storage::bank_reconciliation::verify_audit_source(tx, id, &bytes)
            .map_err(|_| unavailable())?;
    }
    Ok(())
}

/// Reclassify only known pure parser refusals; never storage/source/fence errors.
fn original_parser_error(error: Error) -> Error {
    match error.code {
        "invalid_bank_csv" | "invalid_input" | "invalid_currency" | "invalid_rate" => {
            Error::new("bank_import_invalid_csv", error.message)
        }
        _ => error,
    }
}
#[allow(clippy::too_many_arguments)]
fn import_original(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
    bytes: &[u8],
    account: Option<&str>,
    profile: Option<&str>,
    actor: &str,
    now: &str,
    principal: rentemester_storage::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    rentemester_storage::bank::validate_original_import_account(tx, account)?;
    let account = account.map(str::trim).filter(|s| !s.is_empty());
    let content =
        rentemester_engine::bank::decode_bytes(bytes, profile).map_err(original_parser_error)?;
    let rows = rentemester_engine::bank::parse_with_profile(&content, profile)
        .map_err(original_parser_error)?;
    let archive = guard.publish_bank_statement_bytes(bytes).map_err(|_| {
        Error::new(
            "bank_original_unavailable",
            "bank statement original could not be preserved",
        )
    })?;
    let receipt = rentemester_storage::bank::import_original_bytes_with_principal(
        tx, bytes, &rows, account, actor, now, &archive, principal,
    )?;
    Ok(serde_json::json!({"ok":true,"data":receipt,"errors":[]}))
}
#[cfg(test)]
#[path = "bank/original_tests.rs"]
mod original_tests;

#[cfg(test)]
#[path = "bank/query_tests.rs"]
mod query_tests;

#[cfg(test)]
#[path = "bank/account_tests.rs"]
mod account_tests;
