//! Current actor, invoice originals and archived bank source under one held company/control fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::reminder_claims as storage;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Read(i64),
    Register(&'a storage::Register),
    Post(&'a storage::Post),
    Settle(&'a storage::Settle),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    execute_with_sources(
        control,
        company,
        context,
        action,
        today,
        |tx, id, selected_bank| {
            crate::sales::verify_invoice(tx, guard, id)?;
            let mut banks = storage::required_banks(tx, id)?;
            if let Some(bank) = selected_bank {
                banks.push(bank);
            }
            banks.sort_unstable();
            banks.dedup();
            for bank in banks {
                let hash = rentemester_storage::bank_reconciliation::source_hash(tx, bank)?;
                let bytes = guard.read_bank_statement(&hash).map_err(|_| {
                    Error::new(
                        "bank_statement_original_unavailable",
                        "claim receipt requires its exact archived bank source",
                    )
                })?;
                rentemester_storage::bank_reconciliation::verify_source(tx, bank, &bytes)?;
            }
            Ok(())
        },
    )
}
fn execute_with_sources(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
    mut verify: impl FnMut(&rusqlite::Transaction<'_>, i64, Option<i64>) -> Result<(), Error>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Read(_) => Permission::CompanyRead,
        Action::Register(_) => Permission::CompanyDraftWrite,
        _ => Permission::CompanyLedgerPost,
    };
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
        |tx, actor| {
            let id = match action {
                Action::Read(id) => id,
                Action::Register(i) => i.invoice_document_id,
                Action::Post(i) => i.invoice_document_id,
                Action::Settle(i) => i.invoice_document_id,
            };
            verify(
                tx,
                id,
                if let Action::Settle(i) = action {
                    Some(i.bank_transaction_id)
                } else {
                    None
                },
            )?;
            match action {
                Action::Read(id) => storage::read(tx, id, today),
                Action::Register(i) => storage::register(tx, i, actor),
                Action::Post(i) => storage::post(tx, i, actor, Date::parse(today)?),
                Action::Settle(i) => storage::settle(tx, i, actor, Date::parse(today)?),
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn current_read_draft_post_roles_and_revocation_precede_original_access() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut c,
            &mut l,
            "Synthetic reminder permissions",
        )
        .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');
            INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
            INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookie =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap()
                .verify(fixture["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let context = |slug| Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug,
                registered: true,
                archived: false,
            },
        };

        let input:storage::Register=serde_json::from_value(serde_json::json!({"invoiceDocumentId":7,"reminderDate":"2026-01-03","feeAmount":50,"confirm":true})).unwrap();
        let mut reached = false;
        assert!(
            execute_with_sources(
                &mut c,
                &mut l,
                context("a"),
                Action::Register(&input),
                "2026-01-01",
                |_, _, _| {
                    reached = true;
                    Err(Error::new("synthetic_source_refused", "synthetic"))
                }
            )
            .is_err()
        );
        assert!(
            !reached,
            "reader must not reach source files for registration"
        );
        assert!(
            execute_with_sources(
                &mut c,
                &mut l,
                context("a"),
                Action::Read(7),
                "2026-01-01",
                |_, _, _| {
                    reached = true;
                    Err(Error::new("synthetic_source_refused", "synthetic"))
                }
            )
            .is_err()
        );
        assert!(reached, "reader may read current canonical source");
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','agent:synthetic')").unwrap();
        reached = false;
        assert!(
            execute_with_sources(
                &mut c,
                &mut l,
                context("a"),
                Action::Register(&input),
                "2026-01-01",
                |_, _, _| {
                    reached = true;
                    Err(Error::new(
                        "synthetic_source_refused",
                        "original unavailable",
                    ))
                }
            )
            .is_err()
        );
        assert!(reached);
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic')").unwrap();
        reached = false;
        assert!(
            execute_with_sources(
                &mut c,
                &mut l,
                context("a"),
                Action::Read(7),
                "2026-01-01",
                |_, _, _| {
                    reached = true;
                    Err(Error::new("synthetic_source_refused", "synthetic"))
                }
            )
            .is_err()
        );
        assert!(
            !reached,
            "revoked membership cannot read original/archive files"
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM invoice_reminders", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
