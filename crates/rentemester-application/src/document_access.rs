//! Access evidence is committed before a byte-bearing result can escape.
//! Invalid credentials and object-resolution errors are not authorization-denial events.
use crate::{
    control_fence::{Committed, Failure, FailureStage},
    service_company::{BusinessStage, ServiceFailure, ServiceResult},
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, VerifiedCookie},
    service_key::ServiceKey,
};
use rentemester_storage::{
    cookie_session::{self, SessionRequirement},
    workspace_context::{Catalog, RegisteredCompany},
};
use rusqlite::{Connection, TransactionBehavior};
fn unavailable() -> Error {
    Error::new(
        "document_access_audit_unavailable",
        "document access evidence could not be persisted",
    )
}
fn persist(
    control: &mut Connection,
    slug: &str,
    resource: &str,
    id: i64,
    served: bool,
    resolve: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<(String, bool), Error>,
) -> Result<(), Error> {
    crate::control_fence::connection_ready(control)?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| unavailable())?;
    let (subject, service) = resolve(&tx)?;
    rentemester_storage::document_access::append(
        &tx, &subject, service, slug, resource, id, served,
    )?;
    tx.commit().map_err(|_| unavailable())
}
pub(crate) fn human<T>(
    control: &mut Connection,
    cookie: &VerifiedCookie,
    clock: &dyn Clock,
    slug: &str,
    resource: &str,
    id: i64,
    result: Result<Committed<T>, Failure>,
) -> Result<Committed<T>, Failure> {
    let served = result.is_ok();
    if !served
        && !result
            .as_ref()
            .is_err_and(|e| e.error.code == "access_denied")
    {
        return result;
    }
    let audit = persist(control, slug, resource, id, served, |tx| {
        let time = clock.now_unix_ms().map_err(|_| unavailable())?;
        let identity =
            cookie_session::resolve(tx, cookie.token(), time, SessionRequirement::Current)?;
        Ok((identity.subject, false))
    });
    if let Err(error) = audit {
        return Err(Failure {
            stage: FailureStage::NotCommitted,
            error,
            control_reusable: control.is_autocommit(),
            company_reusable: true,
        });
    }
    result
}
pub(crate) fn service<T>(
    control: &mut Connection,
    key: &ServiceKey,
    clock: &dyn Clock,
    slug: &str,
    registered: bool,
    archived: bool,
    resource: &str,
    id: i64,
    result: Result<ServiceResult<T>, ServiceFailure>,
) -> Result<ServiceResult<T>, ServiceFailure> {
    let served = result.is_ok();
    if !served
        && !result
            .as_ref()
            .is_err_and(|e| e.error.code == "access_denied")
    {
        return result;
    }
    let catalog = Catalog {
        workspace: key.workspace().into(),
        companies: if registered {
            vec![RegisteredCompany {
                slug: slug.into(),
                name: slug.into(),
                purpose: Default::default(), archived,
            }]
        } else {
            vec![]
        },
    };
    let audit = persist(control, slug, resource, id, served, |tx| {
        let time = clock.now_unix_ms().map_err(|_| unavailable())?;
        let observed =
            rentemester_storage::service_context::observe(tx, key.lookup_hash(), time, &catalog)?;
        Ok((observed.subject().into(), true))
    });
    if let Err(error) = audit {
        let usage = match &result {
            Ok(r) => r.usage,
            Err(e) => e.usage,
        };
        return Err(ServiceFailure {
            usage,
            business: BusinessStage::NotAttempted,
            error,
            control_reusable: control.is_autocommit(),
            company_reusable: true,
            rejection: None,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000)
        }
    }
    fn setup() -> (tempfile::TempDir, Connection, Connection, VerifiedCookie) {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut d = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic audit")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');").unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let issued = verifier.issue_session("synthetic-token").unwrap();
        let cookie = verifier
            .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        (dir, c, d, cookie)
    }
    fn served() -> Result<Committed<Vec<u8>>, Failure> {
        Ok(Committed {
            value: b"synthetic exact bytes".to_vec(),
            control_reusable: true,
        })
    }
    #[test]
    fn served_and_denied_are_attributed_without_changing_company() {
        let (_dir, mut c, d, cookie) = setup();
        let before = d.total_changes();
        for resource in ["document_file", "issued_invoice_pdf"] {
            assert_eq!(
                human(&mut c, &cookie, &Fixed, "a", resource, 7, served())
                    .unwrap()
                    .value,
                b"synthetic exact bytes"
            );
            let denied: Result<Committed<Vec<u8>>, Failure> = Err(Failure {
                stage: FailureStage::NotCommitted,
                error: Error::new("access_denied", "refused"),
                control_reusable: true,
                company_reusable: true,
            });
            assert_eq!(
                human(&mut c, &cookie, &Fixed, "foreign", resource, 7, denied)
                    .unwrap_err()
                    .error
                    .code,
                "access_denied"
            );
        }
        assert_eq!(d.total_changes(), before);
        let rows:i64=c.query_row("SELECT count(*) FROM rm_workspace_document_access_events WHERE actor='user:u' AND request_id IS NULL AND ((company_slug='a' AND outcome='served' AND reason_code='authorized') OR(company_slug='foreign' AND outcome='denied' AND reason_code='authorization_denied'))",[],|r|r.get(0)).unwrap();
        assert_eq!(rows, 4);
    }
    #[test]
    fn audit_failure_and_revoked_session_never_release_bytes() {
        let (_dir, mut c, _d, cookie) = setup();
        c.execute_batch("CREATE TRIGGER synthetic_audit_failure BEFORE INSERT ON rm_workspace_document_access_events BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
        assert!(human(&mut c, &cookie, &Fixed, "a", "document_file", 1, served()).is_err());
        c.execute_batch("DROP TRIGGER synthetic_audit_failure;DELETE FROM session;")
            .unwrap();
        assert!(
            human(
                &mut c,
                &cookie,
                &Fixed,
                "a",
                "issued_invoice_pdf",
                1,
                served()
            )
            .is_err()
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM rm_workspace_document_access_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
