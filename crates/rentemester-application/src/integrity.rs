//! Current company-read authorization and owned-transaction hash-chain observation.
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
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let workspace=context.selection.workspace;let clock=context.clock;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead, true, true,
        |control,tx,_| {
            let mut value=rentemester_storage::integrity::read(tx,slug)?;
            if let Ok(status)=crate::backup_status::data(control,tx,workspace,slug,clock,true) {value["backup"]=status["backup"].clone();value["backupLock"]=status["lock"].clone();}
            Ok(value)
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn current_reader_is_admitted_but_membership_and_session_revocation_refuse_before_disclosure() {
        let directory = tempfile::tempdir().unwrap();
        let mut c = Connection::open(directory.path().join("control.sqlite")).unwrap();
        let mut l = Connection::open(directory.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic").unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Secure,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .unwrap();
        let issued = verifier.issue_session("synthetic-token").unwrap();
        let cookie = verifier
            .verify(Some(issued.request_header()), None)
            .unwrap();
        let context = || Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: crate::control_fence::CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let control_before = c.total_changes();
        let ledger_before = l.total_changes();
        let data = read(&mut c, &mut l, context()).unwrap().value;
        assert_eq!(data["auditChain"]["entries"], 0);
        assert_eq!(c.total_changes(), control_before);
        assert_eq!(l.total_changes(), ledger_before);
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor) VALUES('u','a','revoke','agent:synthetic')").unwrap();
        assert_eq!(
            read(&mut c, &mut l, context()).unwrap_err().error.code,
            "access_denied"
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');DELETE FROM session WHERE id='s'").unwrap();
        assert_eq!(
            read(&mut c, &mut l, context()).unwrap_err().error.code,
            "invalid_credentials"
        );
    }
}
