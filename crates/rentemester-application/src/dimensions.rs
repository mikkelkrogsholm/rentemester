//! Current authority remains held across classification preflight and append-only changes.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_storage::dimensions::{AssignmentInput, MasterInput, Supersede};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Definitions,
    Members(Option<&'a str>),
    Assignments(i64),
    Plan(&'a AssignmentInput),
    Master(&'a str, &'a MasterInput),
    Apply(&'a AssignmentInput),
    Replace(&'a AssignmentInput),
    Supersede(&'a Supersede),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Master(..) => Permission::CompanyMasterData,
        Action::Apply(_) => Permission::CompanyDraftWrite,
        Action::Replace(_) | Action::Supersede(_) => Permission::CompanyReview,
        _ => Permission::CompanyRead,
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
        |tx, subject| {
            use rentemester_storage::dimensions as d;
            let actor = format!("user:{subject}");
            match action {
                Action::Definitions => d::definitions(tx),
                Action::Members(dimension) => d::members(tx, dimension),
                Action::Assignments(line) => d::assignments(tx, line),
                Action::Plan(input) => d::plan(tx, input),
                Action::Master(operation, input) => d::master(tx, operation, input, &actor),
                Action::Apply(input) => d::apply(tx, input, false, &actor),
                Action::Replace(input) => d::apply(tx, input, true, &actor),
                Action::Supersede(input) => d::supersede(tx, input, &actor),
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
    fn source_permissions_current_company_and_revocation_gate_atomic_assignments() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Dimensions")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        l.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'A','Synthetic asset','asset','debit'),(2,'E','Synthetic equity','equity','credit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'SYN-1','2026-01-01','Synthetic opening','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(id,journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,1,100.01,0),(2,1,2,0,100.01);").unwrap();
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
        let definition:MasterInput=serde_json::from_value(json!({"confirm":true,"dimensionId":"project","kind":"project","name":"Synthetic project"})).unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Definitions).is_ok());
        assert!(execute(&mut c, &mut l, context("other"), Action::Definitions).is_err());
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::Master("define", &definition)
            )
            .is_err()
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        execute(
            &mut c,
            &mut l,
            context("a"),
            Action::Master("define", &definition),
        )
        .unwrap();
        for m in ["alpha", "beta"] {
            let member: MasterInput = serde_json::from_value(
                json!({"confirm":true,"dimensionId":"project","memberId":m,"name":m}),
            )
            .unwrap();
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::Master("member", &member),
            )
            .unwrap();
        }
        let mut input:AssignmentInput=serde_json::from_value(json!({"confirm":true,"journalLineId":1,"allocations":[{"dimensionId":"project","memberId":"alpha","amountMinor":10001,"currency":"DKK"}]})).unwrap();
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let preview = execute(&mut c, &mut l, context("a"), Action::Plan(&input)).unwrap();
        input.plan_hash = preview.value["plan"]["planHash"]
            .as_str()
            .map(str::to_owned);
        assert!(execute(&mut c, &mut l, context("a"), Action::Apply(&input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        let original = execute(&mut c, &mut l, context("a"), Action::Apply(&input)).unwrap();
        input.allocations[0].member_id = "beta".into();
        input.reason = Some("Synthetic reviewed change".into());
        input.expected_assignment_id = original.value["id"].as_i64();
        input.plan_hash = execute(&mut c, &mut l, context("a"), Action::Plan(&input))
            .unwrap()
            .value["plan"]["planHash"]
            .as_str()
            .map(str::to_owned);
        assert!(execute(&mut c, &mut l, context("a"), Action::Replace(&input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reviewer','agent:synthetic');").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::Master("define", &definition)
            )
            .is_err()
        );
        execute(&mut c, &mut l, context("a"), Action::Replace(&input)).unwrap();
        let before = l.total_changes();
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Replace(&input)).is_err());
        assert!(execute(&mut c, &mut l, context("a"), Action::Assignments(1)).is_err());
        assert_eq!(l.total_changes(), before);
        assert_eq!(
            l.query_row(
                "SELECT COUNT(*) FROM accounting_dimension_assignment_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        assert_eq!(
            l.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            l.query_row(
                "SELECT debit_amount FROM journal_lines WHERE id=1",
                [],
                |r| r.get::<_, f64>(0)
            )
            .unwrap(),
            100.01
        );
    }
}
