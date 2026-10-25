//! GDPR lookups are audited mutations; permission and current session are held through commit.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_storage::gdpr::Input;
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Retention,
    OriginalSubject(
        &'a rentemester_storage::gdpr::original717::SubjectInput,
        bool,
    ),
    OriginalAudit(
        &'a rentemester_storage::gdpr::original717::AuditInput,
        Option<&'a rentemester_runtime::backup_artifact::SigningKey>,
    ),
    Export(&'a Input),
    Erase(&'a Input),
    Discover(&'a Input),
    Forget(&'a rentemester_storage::gdpr::ForgetInput),
    LegalHold(&'a rentemester_storage::gdpr::LegalHoldInput),
    AuditLog(
        &'a rentemester_storage::gdpr::AuditInput,
        &'a rentemester_runtime::backup_artifact::SigningKey,
    ),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    today: &str,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Retention => Permission::CompanyRead,
        Action::OriginalSubject(_, _)
        | Action::OriginalAudit(_, _)
        | Action::Export(_)
        | Action::Discover(_)
        | Action::AuditLog(_, _) => Permission::CompanyExport,
        Action::Erase(_) | Action::Forget(_) | Action::LegalHold(_) => Permission::CompanyAdmin,
    };
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        matches!(action, Action::OriginalAudit(_, _)),
        |tx, actor| match action {
            Action::OriginalSubject(i, discovery) => {
                rentemester_storage::gdpr::original717::subject(
                    tx, slug, i, today, actor, discovery,
                )
            }
            Action::OriginalAudit(i, key) => {
                let payload = rentemester_storage::gdpr::original717::audit_events(tx, i, today)?;
                rentemester_runtime::gdpr_audit::original717::seal(
                    &payload,
                    i.sign_with_ed25519,
                    key,
                )
                .map_err(|e| rentemester_engine::Error::new("gdpr_refused", e))
            }
            Action::Retention => rentemester_storage::retention::read(tx, slug, today),
            Action::Export(i) => rentemester_storage::gdpr::export(tx, slug, i, today, actor),
            Action::Erase(i) => rentemester_storage::gdpr::erase(tx, i, today, actor),
            Action::Discover(i) => rentemester_storage::gdpr::discover(tx, slug, i, today, actor),
            Action::Forget(i) => rentemester_storage::gdpr::forget(tx, i, today, actor),
            Action::LegalHold(i) => rentemester_storage::gdpr::legal_hold(tx, i, today, actor),
            Action::AuditLog(i, key) => {
                let payload = rentemester_storage::gdpr::audit_log(tx, slug, i, today, actor)?;
                rentemester_runtime::gdpr_audit::seal(&payload, key)
                    .map_err(|e| rentemester_engine::Error::new("gdpr_refused", e))
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    use serde_json::json;
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn distinct_permissions_current_company_revocation_and_audit_actor() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic GDPR")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        l.execute_batch("INSERT INTO customers(name,vat_or_cvr,email)VALUES('Synthetic Subject','SYN-GDPR','subject@example.invalid');").unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
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

        let input: Input =
            serde_json::from_value(json!({"cvr":"SYN-GDPR","confirm":true})).unwrap();
        let before = l.total_changes();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Retention
            )
            .is_ok()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("other"),
                "2026-01-01",
                Action::Retention
            )
            .is_err()
        );
        assert_eq!(l.total_changes(), before);
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Export(&input)
            )
            .is_ok()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Erase(&input)
            )
            .is_err()
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        execute(
            &mut c,
            &mut l,
            context("a"),
            "2026-01-01",
            Action::Export(&input),
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Erase(&input)
            )
            .is_err()
        );
        let no: Input = serde_json::from_value(json!({"cvr":"SYN-GDPR","confirm":false})).unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Export(&no)
            )
            .is_err()
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','agent:synthetic');").unwrap();
        execute(
            &mut c,
            &mut l,
            context("a"),
            "2026-01-01",
            Action::Erase(&input),
        )
        .unwrap();
        assert_eq!(
            l.query_row(
                "SELECT actor FROM audit_log WHERE event_type='gdpr_export'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "user:u"
        );
        let held:rentemester_storage::gdpr::LegalHoldInput=serde_json::from_value(json!({"cvr":"SYN-GDPR","holdId":"authority-case","operation":"place","reason":"Synthetic authority test","confirm":true})).unwrap();
        execute(
            &mut c,
            &mut l,
            context("a"),
            "2026-01-01",
            Action::LegalHold(&held),
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("other"),
                "2026-01-01",
                Action::Discover(&input)
            )
            .is_err()
        );
        let forget: rentemester_storage::gdpr::ForgetInput = serde_json::from_value(
            json!({"cvr":"SYN-GDPR","afterRetentionExpiry":true,"confirm":true}),
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Forget(&forget)
            )
            .is_err()
        );
        let original: rentemester_storage::gdpr::original717::SubjectInput =
            serde_json::from_value(json!({"cvr":"SYN-GDPR","confirm":true})).unwrap();
        let original_audit: rentemester_storage::gdpr::original717::AuditInput =
            serde_json::from_value(json!({})).unwrap();
        for discovery in [true, false] {
            assert!(
                execute(
                    &mut c,
                    &mut l,
                    context("a"),
                    "2026-01-01",
                    Action::OriginalSubject(&original, discovery)
                )
                .is_ok()
            );
        }
        let before_audit = l.total_changes();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::OriginalAudit(&original_audit, None)
            )
            .is_ok()
        );
        assert_eq!(l.total_changes(), before_audit);
        assert!(
            execute(
                &mut c,
                &mut l,
                context("other"),
                "2026-01-01",
                Action::OriginalAudit(&original_audit, None)
            )
            .is_err()
        );
        let before = l.total_changes();
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Erase(&input)
            )
            .is_err()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Export(&input)
            )
            .is_err()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::Discover(&input)
            )
            .is_err()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::LegalHold(&held)
            )
            .is_err()
        );
        for discovery in [true, false] {
            assert!(
                execute(
                    &mut c,
                    &mut l,
                    context("a"),
                    "2026-01-01",
                    Action::OriginalSubject(&original, discovery)
                )
                .is_err()
            );
        }
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                "2026-01-01",
                Action::OriginalAudit(&original_audit, None)
            )
            .is_err()
        );
        assert_eq!(l.total_changes(), before);
        assert_eq!(
            l.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
