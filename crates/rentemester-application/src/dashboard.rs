//! Current-read-authorized company overview on the same admitted owned transaction.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    today: &str,
    year: Option<&str>,
    as_of: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        |tx| rentemester_storage::dashboard::read(tx, slug, today, year, as_of),
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
    fn reader_observes_without_effects_wrong_company_and_revocation_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Dashboard")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');
            INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
            INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
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
        let changes = (c.total_changes(), l.total_changes());
        let result = read(&mut c, &mut l, context("a"), "2026-01-01", None, None).unwrap();
        assert_eq!(result.value["company"]["name"], "Synthetic Dashboard");
        assert_eq!((c.total_changes(), l.total_changes()), changes);
        assert!(read(&mut c, &mut l, context("other"), "2026-01-01", None, None).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(read(&mut c, &mut l, context("a"), "2026-01-01", None, None).is_err());
        assert_eq!(l.total_changes(), changes.1);
    }
}
