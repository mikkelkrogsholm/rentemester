//! Queue reads and review-only decisions under current company authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    List(&'a str),
    ListWithArchived(&'a str, bool),
    Resolve(i64, &'a rentemester_storage::exceptions::Resolve),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let permission = match action {
        Action::List(_) | Action::ListWithArchived(..) => Permission::CompanyRead,
        Action::Resolve(..) => Permission::CompanyReview,
    };
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
        matches!(action, Action::List(_) | Action::ListWithArchived(..)),
        |tx, actor| match action {
            Action::List(status) => rentemester_storage::exceptions::list(tx, slug, status),
            Action::ListWithArchived(status, include_archived) => {
                rentemester_storage::exceptions::list_with_archived(
                    tx,
                    slug,
                    status,
                    include_archived,
                )
            }
            Action::Resolve(id, input) => {
                rentemester_storage::exceptions::resolve(tx, id, input, actor)
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
    fn only_current_review_permission_can_resolve_and_revocation_preserves_decision() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Review")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        l.execute_batch("INSERT INTO exceptions(id,type,severity,message) VALUES(1,'SYNTHETIC_REVIEW_REQUIRED','high','Synthetic exception review fixture');").unwrap();
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
        let input = rentemester_storage::exceptions::Resolve { note: None };
        let before = l.total_changes();
        assert_eq!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::ListWithArchived("all", true)
            )
            .unwrap()
            .value["count"],
            1
        );
        assert_eq!(l.total_changes(), before);
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::List("open"))
                .unwrap()
                .value["count"],
            1
        );
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reviewer','agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("other"), Action::Resolve(1, &input)).is_err());
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input))
                .unwrap()
                .value["resolved"],
            true
        );
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input))
                .unwrap()
                .value["resolved"],
            false
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::ListWithArchived("all", true)
            )
            .is_err()
        );
        assert!(execute(&mut c, &mut l, context("a"), Action::List("open")).is_err());
        assert_eq!(l.query_row("SELECT count(*) FROM audit_log WHERE event_type='exception_resolve' AND actor='user:u'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(
            l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
