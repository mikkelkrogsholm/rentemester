//! Deterministic advisory reads under current company read authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
pub use rentemester_storage::bank_matches::Input;
use rusqlite::Connection;
use serde_json::Value;

pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: &Input,
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
        Permission::CompanyRead,
        true,
        |tx, _| rentemester_storage::bank_matches::suggest(tx, input),
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
    fn reader_suggestions_are_effect_free_and_revocation_is_current() {
        let dir = tempfile::tempdir().unwrap();
        let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic Bank Match Read",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookie =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap()
                .verify(fixture["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let context = || Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let input = Input {
            bank_transaction_id: None,
            max: None,
        };
        let before = (control.total_changes(), company.total_changes());
        let result = execute(&mut control, &mut company, context(), &input).unwrap();
        assert_eq!(
            result.value,
            serde_json::json!({"ok":true,"count":0,"rows":[],"errors":[]})
        );
        assert_eq!((control.total_changes(), company.total_changes()), before);
        let missing = execute(
            &mut control,
            &mut company,
            context(),
            &Input {
                bank_transaction_id: Some(7),
                max: Some(1),
            },
        )
        .unwrap();
        assert_eq!(missing.value["ok"], false);
        assert_eq!(
            missing.value["errors"],
            serde_json::json!(["unmatched bank transaction 7 does not exist"])
        );
        assert_eq!((control.total_changes(), company.total_changes()), before);
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(execute(&mut control, &mut company, context(), &input).is_err());
        assert_eq!(company.total_changes(), before.1);
    }
}
