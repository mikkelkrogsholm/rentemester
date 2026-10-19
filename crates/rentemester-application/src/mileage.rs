//! Mileage register under current company authority, without document or journal effects.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    List(Option<&'a str>),
    Register(&'a rentemester_storage::mileage::Register),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::List(_) => Permission::CompanyRead,
        Action::Register(_) => Permission::CompanyLedgerPost,
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
        |tx, actor| match action {
            Action::List(year) => rentemester_storage::mileage::read(tx, slug, today, year),
            Action::Register(input) => rentemester_storage::mileage::register(tx, input, actor),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    use serde_json::json;
    struct FixedClock;
    impl Clock for FixedClock {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn current_membership_archival_and_revocation_fence_mileage() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Mileage")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');
            INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
            INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let corpus: Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let proof =
            CookieVerifier::new("w", CookieProfile::Secure, corpus["key"].as_str().unwrap())
                .unwrap()
                .verify(corpus["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let input=serde_json::from_value(json!({"tripDate":"2026-01-01","purpose":"Synthetic visit","fromLocation":"A","toLocation":"B","kilometers":10,"vehicle":"Synthetic vehicle","driver":"Synthetic driver","ratePerKm":2,"rateBasis":"Synthetic confirmed basis","confirm":true})).unwrap();
        let context = |archived| Context {
            cookie: &proof,
            clock: &FixedClock,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived,
            },
        };
        execute(
            &mut c,
            &mut l,
            context(false),
            Action::List(None),
            "2026-01-01",
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context(false),
                Action::Register(&input),
                "2026-01-01"
            )
            .is_err()
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM mileage_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context(true),
                Action::Register(&input),
                "2026-01-01"
            )
            .is_err()
        );
        let registered = execute(
            &mut c,
            &mut l,
            context(false),
            Action::Register(&input),
            "2026-01-01",
        )
        .unwrap();
        assert_eq!(registered.value["entryNo"], "MIL-2026-000001");
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context(false),
                Action::Register(&input),
                "2026-01-01"
            )
            .is_err()
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context(false),
                Action::List(None),
                "2026-01-01"
            )
            .is_err()
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM mileage_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            l.query_row(
                "SELECT actor FROM audit_log WHERE event_type='mileage_entry_create'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "user:u"
        );
    }
}
